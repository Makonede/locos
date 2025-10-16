pub mod controller;
pub mod registers;
pub mod commands;

pub use controller::{
    NvmeError, NvmeNamespace,
    read_blocks, write_blocks, get_namespaces,
    test_nvme_io,
};

pub fn init() {
    controller::nvme_init();
}
