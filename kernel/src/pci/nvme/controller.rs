//! NVMe controller management
//!
//! This module handles NVMe controller initialization and management,
//! following the same patterns as the xHCI implementation.

use alloc::vec::Vec;
use spin::Mutex;
use x86_64::{PhysAddr, VirtAddr};

use super::{
    commands::{IdentifyController, IdentifyNamespace, NvmeCommand, NvmeCompletion},
    registers::NvmeRegisters,
};
use crate::{
    debug, info,
    memory::FRAME_ALLOCATOR,
    pci::{
        config::device_classes, device::{BarInfo, PciDevice}, dma::{get_zeroed_dma, DmaError, DMA_MANAGER}, msi::{setup_msix, MsiXInfo}, vmm::map_bar, PCI_MANAGER
    },
    tasks::scheduler::kyield_task,
    warn,
};

/// Global NVMe controller instance
pub static NVME_CONTROLLER: Mutex<Option<NvmeController>> = Mutex::new(None);

pub const NVME_VECTOR_BASE: u8 = 0x50;
pub const NVME_ADMIN_VECTOR: u8 = NVME_VECTOR_BASE;
pub const NVME_IO_VECTOR: u8 = NVME_VECTOR_BASE + 1;
pub const NVME_VECTOR_NUM: u16 = 2;

pub fn handle_admin_interrupt() {
    crate::tasks::scheduler::wake_tasks(NVME_ADMIN_VECTOR);
}

pub fn handle_io_interrupt() {
    crate::tasks::scheduler::wake_tasks(NVME_IO_VECTOR);
}

/// NVMe controller errors
#[derive(Debug, Clone, Copy)]
pub enum NvmeError {
    ControllerNotFound,
    ControllerResetTimeout,
    ControllerEnableTimeout,
    QueueFull,
    CommandTimeout,
    CommandNotCompleted,
    CommandFailed(u16),
    AllocationFailed,
    InvalidNamespace,
    PciError,
    NoIoQueue,
    BufferTooSmall,
}

impl From<DmaError> for NvmeError {
    fn from(value: DmaError) -> Self {
        NvmeError::AllocationFailed
    }
}

/// Queue management structure
#[derive(Debug)]
pub struct NvmeQueue {
    /// Submission queue entries
    pub sq_entries: VirtAddr,
    /// Submission queue physical address
    pub sq_phys: PhysAddr,
    /// Completion queue entries
    pub cq_entries: VirtAddr,
    /// Completion queue physical address
    pub cq_phys: PhysAddr,
    /// Queue size (number of entries)
    pub size: u16,
    /// Submission queue head
    pub sq_head: u16,
    /// Submission queue tail
    pub sq_tail: u16,
    /// Completion queue head
    pub cq_head: u16,
    /// Completion queue phase bit
    pub cq_phase: bool,
    /// Queue ID
    pub queue_id: u16,
    /// MSI-X interrupt vector for this queue (None for admin queue using polling)
    pub interrupt_vector: Option<u8>,
}

/// NVMe namespace information
#[derive(Debug, Clone)]
pub struct NvmeNamespace {
    pub nsid: u32,
    pub size_blocks: u64,
    pub block_size: u32,
    pub capacity_blocks: u64,
}

/// Main NVMe controller structure
pub struct NvmeController {
    /// PCIe device information
    pub pci_device: PciDevice,
    /// Memory-mapped registers
    pub registers: &'static mut NvmeRegisters,
    /// Admin queue (queue ID 0)
    pub admin_queue: NvmeQueue,
    /// I/O queue (queue ID 1)
    pub io_queue: Option<NvmeQueue>,
    /// Next command ID to use
    pub next_command_id: u16,
    /// Discovered namespaces
    pub namespaces: Vec<NvmeNamespace>,
    /// Controller capabilities
    pub max_queue_entries: u16,
    pub doorbell_stride: u32,
    /// MSI-X interrupt information
    pub msix_info: Option<MsiXInfo>,
}

impl NvmeQueue {
    /// Create a new queue pair
    pub fn new(queue_id: u16, size: u16) -> Result<Self, NvmeError> {
        let sq_size = size as usize * 64; // 64 bytes per SQ entry
        let cq_size = size as usize * 16; // 16 bytes per CQ entry
        let total_size = sq_size + cq_size;
        let pages_needed = total_size.div_ceil(4096);

        let buffer = get_zeroed_dma(pages_needed)?;
        let sq_virt = buffer.virt_addr;
        let sq_phys = buffer.phys_addr;
        let cq_virt = VirtAddr::new(sq_virt.as_u64() + sq_size as u64);
        let cq_phys = PhysAddr::new(sq_phys.as_u64() + sq_size as u64);

        debug!(
            "Created NVMe queue {}: SQ at {:#x}, CQ at {:#x}",
            queue_id,
            sq_virt.as_u64(),
            cq_virt.as_u64()
        );

        Ok(Self {
            sq_entries: sq_virt,
            sq_phys,
            cq_entries: cq_virt,
            cq_phys,
            size,
            sq_head: 0,
            sq_tail: 0,
            cq_head: 0,
            cq_phase: true,
            queue_id,
            interrupt_vector: None,
        })
    }

    /// Submit a command to the submission queue
    pub fn submit_command(&mut self, mut cmd: NvmeCommand) -> Result<u16, NvmeError> {
        let next_tail = (self.sq_tail + 1) % self.size;
        if next_tail == self.sq_head {
            return Err(NvmeError::QueueFull);
        }

        let cid = self.sq_tail;
        cmd.set_command_id(cid);

        unsafe {
            let entry_ptr = self
                .sq_entries
                .as_mut_ptr::<NvmeCommand>()
                .add(self.sq_tail as usize);
            core::ptr::write_volatile(entry_ptr, cmd);
        }

        self.sq_tail = next_tail;

        Ok(cid)
    }

    /// Check for completion queue entries
    pub fn check_completion(&mut self) -> Option<NvmeCompletion> {
        let entry_ptr = unsafe {
            self.cq_entries
                .as_ptr::<NvmeCompletion>()
                .add(self.cq_head as usize)
        };

        let completion = unsafe { core::ptr::read_volatile(entry_ptr) };

        if completion.is_valid(self.cq_phase) {
            self.cq_head = (self.cq_head + 1) % self.size;

            if self.cq_head == 0 {
                self.cq_phase = !self.cq_phase;
            }

            Some(completion)
        } else {
            None
        }
    }
}

impl NvmeController {
    /// Find and initialize the first NVMe controller
    pub fn new(pci_device: PciDevice) -> Result<Self, NvmeError> {
        info!(
            "Initializing NVMe controller: {:02x}:{:02x}.{} [{:04x}:{:04x}]",
            pci_device.bus,
            pci_device.device,
            pci_device.function,
            pci_device.vendor_id,
            pci_device.device_id
        );

        let memory_bar = pci_device
            .bars
            .iter()
            .find_map(|bar| {
                if let BarInfo::Memory(memory_bar) = bar {
                    Some(memory_bar)
                } else {
                    None
                }
            })
            .ok_or(NvmeError::PciError)?;

        let mapped_bar = map_bar(memory_bar).map_err(|_| NvmeError::PciError)?;
        let registers = unsafe { NvmeRegisters::new(mapped_bar.virtual_address) };

        debug!(
            "NVMe registers mapped at {:#x}",
            mapped_bar.virtual_address.as_u64()
        );

        let max_queue_entries = registers.max_queue_entries();
        let doorbell_stride = registers.doorbell_stride();

        debug!("NVMe Controller Capabilities:");
        debug!("  Max Queue Entries: {}", max_queue_entries);
        debug!("  Doorbell Stride: {} bytes", doorbell_stride);
        debug!("  Min Page Size: {} bytes", registers.min_page_size());
        debug!("  Max Page Size: {} bytes", registers.max_page_size());

        let admin_queue = NvmeQueue::new(0, core::cmp::min(max_queue_entries, 64))?;

        let mut controller = Self {
            pci_device,
            registers,
            admin_queue,
            io_queue: None,
            next_command_id: 1,
            namespaces: Vec::new(),
            max_queue_entries,
            doorbell_stride,
            msix_info: None,
        };

        controller.initialize()?;

        Ok(controller)
    }

    /// Initialize the NVMe controller
    fn initialize(&mut self) -> Result<(), NvmeError> {
        info!("Initializing NVMe controller");

        if self.registers.is_ready() {
            self.reset_controller()?;
        }

        self.setup_msix()?;

        self.setup_admin_queues()?;

        self.enable_controller()?;

        self.identify_controller()?;

        self.discover_namespaces()?;

        if !self.namespaces.is_empty() {
            self.create_io_queues()?;
        }

        info!("NVMe controller initialization complete");
        Ok(())
    }

    /// Setup MSI-X interrupts for the controller
    fn setup_msix(&mut self) -> Result<(), NvmeError> {
        let mut msix_info = setup_msix(&self.pci_device, NVME_VECTOR_NUM, NVME_VECTOR_BASE)
            .map_err(|_| NvmeError::PciError)?;

        info!(
            "MSI-X enabled for NVMe controller with {} vectors (base={:#x})",
            NVME_VECTOR_NUM, NVME_VECTOR_BASE
        );

        msix_info
            .enable_vector(0)
            .map_err(|_| NvmeError::PciError)?;
        msix_info
            .enable_vector(1)
            .map_err(|_| NvmeError::PciError)?;

        self.msix_info = Some(msix_info);
        Ok(())
    }

    /// Reset the NVMe controller
    fn reset_controller(&mut self) -> Result<(), NvmeError> {
        info!("Resetting NVMe controller");

        self.registers.disable();

        let timeout = 100000; // Busy wait iterations
        for _ in 0..timeout {
            if !self.registers.is_ready() {
                break;
            }
            // Small delay to avoid overwhelming the controller
            for _ in 0..1000 {
                core::hint::spin_loop();
            }
        }

        if self.registers.is_ready() {
            return Err(NvmeError::ControllerResetTimeout);
        }

        info!("Controller reset complete");
        Ok(())
    }

    /// Set up admin submission and completion queues
    fn setup_admin_queues(&mut self) -> Result<(), NvmeError> {
        info!("Setting up admin queues");

        let sq_phys = PhysAddr::new(
            self.admin_queue.sq_entries.as_u64()
                - FRAME_ALLOCATOR.lock().as_ref().unwrap().hddm_offset,
        );
        let cq_phys = PhysAddr::new(
            self.admin_queue.cq_entries.as_u64()
                - FRAME_ALLOCATOR.lock().as_ref().unwrap().hddm_offset,
        );

        self.registers
            .set_admin_queue_attributes(self.admin_queue.size, self.admin_queue.size);

        self.registers.set_admin_sq_base(sq_phys.as_u64());
        self.registers.set_admin_cq_base(cq_phys.as_u64());

        info!(
            "Admin queues configured: SQ={:#x}, CQ={:#x}",
            sq_phys.as_u64(),
            cq_phys.as_u64()
        );
        Ok(())
    }

    /// Enable the NVMe controller
    fn enable_controller(&mut self) -> Result<(), NvmeError> {
        info!("Enabling NVMe controller");

        self.registers.configure();

        let timeout = 100000; // Busy wait iterations
        for _ in 0..timeout {
            if self.registers.is_ready() {
                info!("Controller enabled and ready");
                return Ok(());
            }
            // Small delay
            for _ in 0..1000 {
                core::hint::spin_loop();
            }
        }

        Err(NvmeError::ControllerEnableTimeout)
    }

    /// Submit an admin command and yield to scheduler for completion
    ///
    /// will issue msi-x interrupt when command completes
    fn submit_admin_command(&mut self, cmd: NvmeCommand) -> Result<NvmeCompletion, NvmeError> {
        // Submit command to admin queue
        let cid = self.admin_queue.submit_command(cmd)?;

        self.registers
            .ring_doorbell(0, false, self.admin_queue.sq_tail);

        kyield_task(NVME_ADMIN_VECTOR);

        let completion = self
            .admin_queue
            .check_completion()
            .ok_or(NvmeError::CommandNotCompleted)?;

        if !completion.is_success() {
            return Err(NvmeError::CommandFailed(completion.status_code()));
        }

        Ok(completion)
    }

    /// Identify the controller and get basic information
    fn identify_controller(&mut self) -> Result<(), NvmeError> {
        info!("Identifying NVMe controller");

        let buffer = DMA_MANAGER.lock().get_pool_4kb().ok_or(NvmeError::AllocationFailed)?;

        let cmd = NvmeCommand::identify_controller(buffer.phys_addr.as_u64());
        let _completion = self.submit_admin_command(cmd)?;

        let identify_data = unsafe { &*(buffer.virt_addr.as_ptr::<IdentifyController>()) };

        let model = core::str::from_utf8(&identify_data.mn)
            .unwrap_or("Unknown")
            .trim_end_matches('\0')
            .trim();
        let serial = core::str::from_utf8(&identify_data.sn)
            .unwrap_or("Unknown")
            .trim_end_matches('\0')
            .trim();
        let firmware = core::str::from_utf8(&identify_data.fr)
            .unwrap_or("Unknown")
            .trim_end_matches('\0')
            .trim();

        info!("Controller Information:");
        info!("  Model: {}", model);
        info!("  Serial: {}", serial);
        info!("  Firmware: {}", firmware);
        info!("  Version: {:#x}", identify_data.ver);
        info!("  Namespaces: {}", identify_data.nn);

        Ok(())
    }

    /// Discover and identify namespaces
    fn discover_namespaces(&mut self) -> Result<(), NvmeError> {
        info!("Discovering namespaces");

        match self.identify_namespace(1) {
            Ok(namespace) => {
                self.namespaces.push(namespace);
                info!("Added namespace 1");
            }
            Err(e) => {
                debug!("Namespace 1 not available: {:?}", e);
            }
        }

        info!("Found {} namespace(s)", self.namespaces.len());
        Ok(())
    }

    /// Identify a specific namespace
    fn identify_namespace(&mut self, nsid: u32) -> Result<NvmeNamespace, NvmeError> {
        debug!("Identifying namespace {}", nsid);

        let buffer = get_zeroed_dma(1)?;

        let cmd = NvmeCommand::identify_namespace(nsid, buffer.phys_addr.as_u64());

        let _completion = self.submit_admin_command(cmd)?;

        let identify_data = unsafe { &*(buffer.virt_addr.as_ptr::<IdentifyNamespace>()) };

        if identify_data.nsze == 0 {
            return Err(NvmeError::InvalidNamespace);
        }

        let block_size = identify_data.lba_size();
        let size_blocks = identify_data.nsze;
        let capacity_blocks = identify_data.ncap;

        info!("Namespace {} Information:", nsid);
        info!(
            "  Size: {} blocks ({} MB)",
            size_blocks,
            (size_blocks * block_size as u64) / (1024 * 1024)
        );
        info!("  Block Size: {} bytes", block_size);
        info!("  Capacity: {} blocks", capacity_blocks);

        Ok(NvmeNamespace {
            nsid,
            size_blocks,
            block_size,
            capacity_blocks,
        })
    }

    /// Create I/O submission and completion queues
    fn create_io_queues(&mut self) -> Result<(), NvmeError> {
        info!("Creating I/O queues");

        let queue_size = core::cmp::min(self.max_queue_entries, 64);
        let mut io_queue = NvmeQueue::new(1, queue_size)?;

        let msix_info = self.msix_info.as_ref().ok_or(NvmeError::PciError)?;

        let io_vector = msix_info.vectors.get(1).ok_or(NvmeError::PciError)?;

        io_queue.interrupt_vector = Some(io_vector.vector);

        info!(
            "Creating I/O Completion Queue with MSI-X interrupt vector {:#x}",
            io_vector.vector
        );
        let create_cq_cmd = NvmeCommand::create_io_cq_with_interrupt(
            1,
            queue_size,
            io_queue.cq_phys.as_u64(),
            io_vector.index,
        );

        self.submit_admin_command(create_cq_cmd)?;
        info!("I/O Completion Queue created");

        let create_sq_cmd = NvmeCommand::create_io_sq(1, 1, queue_size, io_queue.sq_phys.as_u64());
        self.submit_admin_command(create_sq_cmd)?;
        info!("I/O Submission Queue created");

        self.io_queue = Some(io_queue);
        info!("I/O queues ready");
        Ok(())
    }

    /// Submit an I/O command and yield current task to scheduler for completion
    ///
    /// Controller will issue an msi-x interrupt when ths command complete
    /// The interrupt vector is configured in the I/O completion queue.
    fn submit_io_command(&mut self, cmd: NvmeCommand) -> Result<NvmeCompletion, NvmeError> {
        let io_queue = self.io_queue.as_mut().ok_or(NvmeError::NoIoQueue)?;

        let cid = io_queue.submit_command(cmd)?;

        self.registers.ring_doorbell(1, false, io_queue.sq_tail);

        kyield_task(NVME_IO_VECTOR);

        let completion = io_queue
            .check_completion()
            .ok_or(NvmeError::CommandNotCompleted)?;

        if !completion.is_success() {
            return Err(NvmeError::CommandFailed(completion.status_code()));
        }

        Ok(completion)
    }

    /// Read blocks from a namespace
    pub fn read_blocks(
        &mut self,
        nsid: u32,
        lba: u64,
        blocks: u16,
        buffer: &mut [u8],
    ) -> Result<(), NvmeError> {
        if !self.namespaces.iter().any(|ns| ns.nsid == nsid) {
            return Err(NvmeError::InvalidNamespace);
        }

        let namespace = self.namespaces.iter().find(|ns| ns.nsid == nsid).unwrap();
        let required_size = blocks as usize * namespace.block_size as usize;

        if buffer.len() < required_size {
            return Err(NvmeError::BufferTooSmall);
        }

        let pages_needed = (required_size + 4095) / 4096;
        let dma_buffer = get_zeroed_dma(pages_needed)?;

        let cmd = NvmeCommand::read(nsid, lba, blocks, dma_buffer.phys_addr.as_u64());
        self.submit_io_command(cmd)?;

        unsafe {
            core::ptr::copy_nonoverlapping(
                dma_buffer.virt_addr.as_ptr::<u8>(),
                buffer.as_mut_ptr(),
                required_size,
            );
        }

        debug!(
            "Read {} blocks from LBA {} (namespace {})",
            blocks, lba, nsid
        );
        Ok(())
    }

    /// Write blocks to a namespace
    pub fn write_blocks(
        &mut self,
        nsid: u32,
        lba: u64,
        blocks: u16,
        buffer: &[u8],
    ) -> Result<(), NvmeError> {
        if !self.namespaces.iter().any(|ns| ns.nsid == nsid) {
            return Err(NvmeError::InvalidNamespace);
        }

        let namespace = self.namespaces.iter().find(|ns| ns.nsid == nsid).unwrap();
        let required_size = blocks as usize * namespace.block_size as usize;

        if buffer.len() < required_size {
            return Err(NvmeError::BufferTooSmall);
        }

        let pages_needed = (required_size + 4095) / 4096;
        let dma_buffer = get_zeroed_dma(pages_needed)?;

        unsafe {
            core::ptr::copy_nonoverlapping(
                buffer.as_ptr(),
                dma_buffer.virt_addr.as_mut_ptr::<u8>(),
                required_size,
            );
        }

        let cmd = NvmeCommand::write(nsid, lba, blocks, dma_buffer.phys_addr.as_u64());
        self.submit_io_command(cmd)?;

        debug!(
            "Wrote {} blocks to LBA {} (namespace {})",
            blocks, lba, nsid
        );
        Ok(())
    }
}

/// Find NVMe controllers (similar to find_xhci_devices)
#[allow(clippy::let_and_return)]
pub fn find_nvme_controllers() -> Vec<PciDevice> {
    let lock = PCI_MANAGER.lock();
    let manager = lock.as_ref().unwrap();

    let nvme_devices: Vec<PciDevice> = manager
        .devices
        .iter()
        .filter(|d| {
            d.class_code == device_classes::MASS_STORAGE && d.subclass == 0x08 && d.prog_if == 0x02
        })
        .cloned()
        .collect();

    info!("Found {} NVMe controller(s)", nvme_devices.len());
    nvme_devices
}

/// Initialize NVMe subsystem (main entry point)
pub fn nvme_init() {
    let controllers = find_nvme_controllers();

    if controllers.is_empty() {
        info!("No NVMe controllers found");
        return;
    }

    match NvmeController::new(controllers[0].clone()) {
        Ok(controller) => {
            info!("NVMe controller initialized successfully");
            *NVME_CONTROLLER.lock() = Some(controller);
        }
        Err(e) => {
            warn!("Failed to initialize NVMe controller: {:?}", e);
        }
    }
}

/// Read blocks from the NVMe device
///
/// # Arguments
/// * `nsid` - Namespace ID (typically 1 for the first namespace)
/// * `lba` - Logical Block Address to start reading from
/// * `blocks` - Number of blocks to read
/// * `buffer` - Buffer to read data into
pub fn read_blocks(nsid: u32, lba: u64, blocks: u16, buffer: &mut [u8]) -> Result<(), NvmeError> {
    let mut controller = NVME_CONTROLLER.lock();
    let controller = controller.as_mut().ok_or(NvmeError::ControllerNotFound)?;
    controller.read_blocks(nsid, lba, blocks, buffer)
}

/// Write blocks to the NVMe device
///
/// # Arguments
/// * `nsid` - Namespace ID (typically 1 for the first namespace)
/// * `lba` - Logical Block Address to start writing to
/// * `blocks` - Number of blocks to write
/// * `buffer` - Buffer containing data to write
pub fn write_blocks(nsid: u32, lba: u64, blocks: u16, buffer: &[u8]) -> Result<(), NvmeError> {
    let mut controller = NVME_CONTROLLER.lock();
    let controller = controller.as_mut().ok_or(NvmeError::ControllerNotFound)?;
    controller.write_blocks(nsid, lba, blocks, buffer)
}

/// Get information about available namespaces
pub fn get_namespaces() -> Vec<NvmeNamespace> {
    let controller = NVME_CONTROLLER.lock();
    if let Some(controller) = controller.as_ref() {
        controller.namespaces.clone()
    } else {
        Vec::new()
    }
}

/// Test NVMe read/write functionality
///
/// This function performs a simple test:
/// 1. Reads a block from LBA 0
/// 2. Writes a test pattern to LBA 1
/// 3. Reads back LBA 1 to verify the write
pub fn test_nvme_io() -> Result<(), NvmeError> {
    info!("Starting NVMe I/O test");

    let namespaces = get_namespaces();
    if namespaces.is_empty() {
        warn!("No NVMe namespaces available for testing");
        return Err(NvmeError::InvalidNamespace);
    }

    let ns = &namespaces[0];
    info!(
        "Testing with namespace {}, block size: {} bytes",
        ns.nsid, ns.block_size
    );

    // Allocate buffers
    let block_size = ns.block_size as usize;
    let mut read_buffer = alloc::vec![0u8; block_size];
    let mut write_buffer = alloc::vec![0u8; block_size];
    let mut verify_buffer = alloc::vec![0u8; block_size];

    // Test 1: Read from LBA 0
    info!("Test 1: Reading from LBA 0");
    read_blocks(ns.nsid, 0, 1, &mut read_buffer)?;
    info!("Successfully read {} bytes from LBA 0", block_size);

    // Display first 64 bytes
    info!("First 64 bytes of LBA 0:");
    for i in (0..64.min(block_size)).step_by(16) {
        let end = (i + 16).min(block_size);
        let hex_str: alloc::string::String = read_buffer[i..end]
            .iter()
            .map(|b| alloc::format!("{:02x} ", b))
            .collect();
        info!("  {:04x}: {}", i, hex_str);
    }

    // Test 2: Write test pattern to LBA 1
    info!("Test 2: Writing test pattern to LBA 1");
    for i in 0..block_size {
        write_buffer[i] = (i % 256) as u8;
    }
    write_blocks(ns.nsid, 1, 1, &write_buffer)?;
    info!("Successfully wrote {} bytes to LBA 1", block_size);

    // Test 3: Read back and verify
    info!("Test 3: Reading back LBA 1 to verify");
    read_blocks(ns.nsid, 1, 1, &mut verify_buffer)?;

    let mut mismatches = 0;
    for i in 0..block_size {
        if write_buffer[i] != verify_buffer[i] {
            mismatches += 1;
            if mismatches <= 10 {
                warn!(
                    "Mismatch at offset {}: wrote {:02x}, read {:02x}",
                    i, write_buffer[i], verify_buffer[i]
                );
            }
        }
    }

    if mismatches == 0 {
        info!("✓ Verification successful! All {} bytes match", block_size);
    } else {
        warn!("✗ Verification failed! {} mismatches found", mismatches);
        return Err(NvmeError::CommandFailed(0));
    }

    info!("NVMe I/O test completed successfully");
    Ok(())
}
