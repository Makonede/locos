pub mod controller;
pub mod registers;
pub mod commands;

pub use controller::{
    NvmeError, NvmeNamespace,
    read_blocks, write_blocks, get_namespaces,
    test_nvme_io,
    handle_admin_interrupt, handle_io_interrupt,
    NVME_VECTOR_BASE, NVME_VECTOR_NUM, NVME_ADMIN_VECTOR, NVME_IO_VECTOR,
};

pub fn init() {
    controller::nvme_init();
}
