use std::rc::Rc;

use lenso_kernel::RuntimeFailure;
use lenso_native_adapter::{NativeModuleFactory, NativeModuleFactoryContext, NativeModuleInstance};

pub const PRODUCER_PACKAGE_ID: &str = "fixture.story-producer";
pub const READER_PACKAGE_ID: &str = "fixture.story-reader";
pub const DENIED_PACKAGE_ID: &str = "fixture.story-denied";

#[derive(Debug)]
pub struct NoopFactory {
    package_id: &'static str,
}

impl NoopFactory {
    pub fn new(package_id: &'static str) -> Self {
        Self { package_id }
    }
}

impl NativeModuleFactory for NoopFactory {
    fn package_id(&self) -> &'static str {
        self.package_id
    }

    fn instantiate(
        &self,
        _context: NativeModuleFactoryContext<'_>,
    ) -> Result<NativeModuleInstance, RuntimeFailure> {
        Ok(NativeModuleInstance::new(Vec::<
            Rc<dyn lenso_kernel::NativeRequestEndpoint>,
        >::new()))
    }
}
