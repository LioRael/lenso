use std::sync::{Arc, atomic::AtomicBool};

use crate::{
    protocol::{WireEventPublish, WireRequest, WireStreamOpen},
    server::BunRequest,
};

pub(crate) trait IntoBunRequest {
    fn into_bun_request(self, cancellation: Arc<AtomicBool>) -> BunRequest;
}

impl IntoBunRequest for WireRequest {
    fn into_bun_request(self, cancellation: Arc<AtomicBool>) -> BunRequest {
        BunRequest {
            request_id: self.request_id,
            capability_id: self.capability_id,
            operation: self.operation,
            deadline_nanos: self.deadline_nanos,
            caller_instance: self.caller_instance,
            payload: self.payload,
            extensions: self.extensions,
            cancellation,
        }
    }
}

impl IntoBunRequest for WireEventPublish {
    fn into_bun_request(self, cancellation: Arc<AtomicBool>) -> BunRequest {
        BunRequest {
            request_id: self.request_id,
            capability_id: self.capability_id,
            operation: self.operation,
            deadline_nanos: self.deadline_nanos,
            caller_instance: self.caller_instance,
            payload: self.payload,
            extensions: self.extensions,
            cancellation,
        }
    }
}

impl IntoBunRequest for WireStreamOpen {
    fn into_bun_request(self, cancellation: Arc<AtomicBool>) -> BunRequest {
        BunRequest {
            request_id: self.request_id,
            capability_id: self.capability_id,
            operation: self.operation,
            deadline_nanos: self.deadline_nanos,
            caller_instance: self.caller_instance,
            payload: self.payload,
            extensions: self.extensions,
            cancellation,
        }
    }
}

impl BunRequest {
    pub(crate) fn from_wire<W: IntoBunRequest>(wire: W, cancellation: Arc<AtomicBool>) -> Self {
        wire.into_bun_request(cancellation)
    }
}
