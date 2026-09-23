use wasmtime::{ResourceLimiter, StoreLimits, StoreLimitsBuilder};

/// A Store-wide cap on the sum of all non-shared Guest linear-memory growth.
/// Wasmtime is built without its `threads` feature, so shared memories cannot
/// bypass `ResourceLimiter::memory_growing` in this Adapter.
#[derive(Debug)]
pub(crate) struct GuestLinearMemoryBudget {
    limits: StoreLimits,
    max_total_bytes: usize,
    reserved_bytes: usize,
}

impl GuestLinearMemoryBudget {
    pub(crate) fn new(
        max_total_bytes: usize,
        max_table_elements: usize,
        max_instances: usize,
    ) -> Self {
        Self {
            limits: StoreLimitsBuilder::new()
                .memory_size(max_total_bytes)
                .table_elements(max_table_elements)
                .instances(max_instances)
                .memories(max_instances)
                .tables(max_instances)
                .trap_on_grow_failure(true)
                .build(),
            max_total_bytes,
            reserved_bytes: 0,
        }
    }
}

impl ResourceLimiter for GuestLinearMemoryBudget {
    fn memory_growing(
        &mut self,
        current: usize,
        desired: usize,
        maximum: Option<usize>,
    ) -> wasmtime::Result<bool> {
        if !self.limits.memory_growing(current, desired, maximum)? {
            return Ok(false);
        }
        let Some(next_total) = desired
            .checked_sub(current)
            .and_then(|growth| self.reserved_bytes.checked_add(growth))
        else {
            return Err(wasmtime::Error::msg("invalid Guest linear-memory growth"));
        };
        if next_total > self.max_total_bytes {
            return Err(wasmtime::Error::msg(
                "Guest aggregate linear-memory ceiling exceeded",
            ));
        }
        // A failed Wasmtime allocation can over-reserve until this Store is
        // destroyed. That is conservative: it cannot admit extra Guest bytes.
        self.reserved_bytes = next_total;
        Ok(true)
    }

    fn table_growing(
        &mut self,
        current: usize,
        desired: usize,
        maximum: Option<usize>,
    ) -> wasmtime::Result<bool> {
        self.limits.table_growing(current, desired, maximum)
    }

    fn memory_grow_failed(&mut self, error: wasmtime::Error) -> wasmtime::Result<()> {
        self.limits.memory_grow_failed(error)
    }

    fn table_grow_failed(&mut self, error: wasmtime::Error) -> wasmtime::Result<()> {
        self.limits.table_grow_failed(error)
    }

    fn instances(&self) -> usize {
        self.limits.instances()
    }

    fn tables(&self) -> usize {
        self.limits.tables()
    }

    fn memories(&self) -> usize {
        self.limits.memories()
    }
}

#[cfg(test)]
mod tests {
    use super::GuestLinearMemoryBudget;
    use wasmtime::{Config, Engine, Memory, MemoryType, Module, Store};

    #[test]
    fn two_guest_memories_share_one_growth_budget() {
        let engine = Engine::new(&Config::new()).unwrap();
        let mut store = Store::new(&engine, GuestLinearMemoryBudget::new(3 * 65_536, 16, 4));
        store.limiter(|budget| budget);
        let memory_type = MemoryType::new(1, None);
        let first = Memory::new(&mut store, memory_type.clone()).unwrap();
        let second = Memory::new(&mut store, memory_type).unwrap();
        assert_eq!(first.size(&store), 1);
        assert_eq!(second.size(&store), 1);
        first.grow(&mut store, 1).unwrap();
        assert!(second.grow(&mut store, 1).is_err());
        assert_eq!(first.size(&store), 2);
        assert_eq!(second.size(&store), 1);
    }

    #[test]
    fn shared_memory_cannot_bypass_the_limiter_in_this_engine() {
        let mut config = Config::new();
        config.wasm_component_model(true);
        let engine = Engine::new(&config).unwrap();
        // One core Wasm memory with the shared flag and an explicit maximum.
        let shared_memory_module = [0, 97, 115, 109, 1, 0, 0, 0, 5, 4, 1, 3, 1, 1];
        assert!(Module::new(&engine, shared_memory_module).is_err());
    }
}
