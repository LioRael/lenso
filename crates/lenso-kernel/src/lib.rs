//! Portable Lenso vNext Kernel and Runtime Driver seam.

use std::{
    any::Any,
    cell::{Cell, RefCell},
    collections::{BTreeMap, BTreeSet, VecDeque},
    future::Future,
    marker::PhantomData,
    panic::AssertUnwindSafe,
    pin::Pin,
    rc::{Rc, Weak},
    task::{Context, Poll},
    time::Duration,
};

use futures::{
    channel::oneshot,
    executor::{LocalPool, LocalSpawner},
    future::{AbortHandle, Abortable, Either, FutureExt, LocalBoxFuture, pending, select},
    task::{LocalSpawnExt, SpawnError},
};
use lenso_app_plan::{
    EventAdmissionPlan, ExecutionClassId, ModuleCriticality, PlanResolutionError,
    RequestAdmissionPlan, ResolvedAppPlan, RestartMode, RestartPolicy,
};

mod deterministic;
mod driver;
mod event;
mod kernel;
mod lifecycle;
mod prepared;
mod request;
mod runtime;
mod stream;
mod supervision;

pub use event::{
    EventAdmission, EventCapability, EventPublishResult, EventPublishStatus,
    ModuleEventDependencyHandle, NativeEventEndpoint, NativeEventHandle,
};
pub use stream::{
    NativeStream, NativeStreamEndpoint, NativeStreamHandle, NativeStreamItem, NativeStreamSession,
    StreamCapability, StreamEvent, StreamSession,
};

type ErasedValue = Box<dyn Any>;
type ErasedDomainResult = Result<ErasedValue, ErasedValue>;
type NativeBindingTable = BTreeMap<(String, &'static str), Vec<NativeEndpointBinding>>;
type NativeEndpointStateTable = BTreeMap<(String, String), Rc<NativeEndpointState>>;
type NativeStreamBindingTable = BTreeMap<(String, &'static str), Vec<NativeStreamEndpointBinding>>;
type NativeStreamEndpointStateTable = BTreeMap<(String, String), Rc<NativeStreamEndpointState>>;
type NativeEventBindingTable =
    BTreeMap<(String, &'static str), Vec<event::NativeEventEndpointBinding>>;
type NativeEventEndpointStateTable =
    BTreeMap<(String, String), Rc<event::NativeEventEndpointState>>;

/// Static identity and Rust value types generated for one request Capability.
pub use deterministic::*;
pub use driver::*;
pub use kernel::*;
pub use lifecycle::*;
pub use prepared::*;
pub use request::*;
pub use runtime::*;
use supervision::{
    begin_module_supervision, deactivate_in_reverse, module_supervision,
    schedule_module_supervision, shutdown_native_modules, validate_native_endpoint_set,
};
