//! Runtime-neutral authoring primitives for strongly typed Lenso Plugins.

use std::{cell::OnceCell, marker::PhantomData, ops::Deref, rc::Rc};

/// Generated exact Capability reference used by dependency declarations.
///
/// It carries contract identity and a zero-sized client type witness without
/// selecting a provider or depending on any product-specific SDK.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CapabilityReference<C> {
    capability_id: &'static str,
    descriptor_version: &'static str,
    descriptor_digest: &'static str,
    client: PhantomData<fn() -> C>,
}

impl<C> CapabilityReference<C> {
    #[must_use]
    pub const fn new(
        capability_id: &'static str,
        descriptor_version: &'static str,
        descriptor_digest: &'static str,
    ) -> Self {
        Self {
            capability_id,
            descriptor_version,
            descriptor_digest,
            client: PhantomData,
        }
    }

    #[must_use]
    pub const fn capability_id(&self) -> &'static str {
        self.capability_id
    }

    #[must_use]
    pub const fn descriptor_version(&self) -> &'static str {
        self.descriptor_version
    }

    #[must_use]
    pub const fn descriptor_digest(&self) -> &'static str {
        self.descriptor_digest
    }
}

/// One Plugin operation failure with an explicit Domain/Runtime split.
///
/// Ordinary operations can return `Result<T, DomainError>` directly. Use this
/// type only when Plugin code must deliberately surface an Adapter-specific
/// runtime failure in addition to its Capability-defined Domain Errors.
#[derive(Clone, Debug, PartialEq)]
pub enum PluginError<DomainError, RuntimeError> {
    /// An expected Capability-defined business rejection.
    Domain(DomainError),
    /// An infrastructure or execution failure outside the Capability contract.
    Runtime(RuntimeError),
}

impl<DomainError, RuntimeError> PluginError<DomainError, RuntimeError> {
    /// Creates a Capability-defined Domain Error.
    pub const fn domain(error: DomainError) -> Self {
        Self::Domain(error)
    }

    /// Creates an Adapter-specific Runtime Error.
    pub const fn runtime(error: RuntimeError) -> Self {
        Self::Runtime(error)
    }

    /// Maps the Domain Error while preserving the Runtime Error.
    pub fn map_domain<Other>(
        self,
        map: impl FnOnce(DomainError) -> Other,
    ) -> PluginError<Other, RuntimeError> {
        match self {
            Self::Domain(error) => PluginError::Domain(map(error)),
            Self::Runtime(error) => PluginError::Runtime(error),
        }
    }
}

/// A generated, strongly typed client for one required Capability.
///
/// Capability binding generators implement this trait for their client type so
/// Plugin authoring frontends can connect typed Ports without knowing the
/// Capability's operation kinds or handle layout. Implementations must use only
/// the supplied Plan-owned dependencies; they must not perform discovery.
pub trait CapabilityClient: Sized + 'static {
    /// Adapter-owned dependency view used to connect this client.
    type Dependencies: ?Sized;
    /// Adapter-owned failure returned when connection cannot complete.
    type Error;

    /// Stable Capability identity required by this client.
    const CAPABILITY_ID: &'static str;
    /// Exact Descriptor version understood by this generated client.
    const DESCRIPTOR_VERSION: &'static str;

    /// Connects this client to one Plugin Instance's resolved dependencies.
    fn from_dependencies(dependencies: &Self::Dependencies) -> Result<Self, Self::Error>;

    /// Connects this client to one source-named requirement.
    ///
    /// Generated clients override this method to narrow the dependency view by
    /// exact requirement identity. The default preserves older projections.
    fn from_requirement(
        dependencies: &Self::Dependencies,
        _requirement_id: &str,
    ) -> Result<Self, Self::Error> {
        Self::from_dependencies(dependencies)
    }

    /// Creates the adapter failure for an invalid second connection attempt.
    fn already_connected() -> Self::Error;
}

/// A generated Capability client that can be connected to every explicitly
/// bound provider in deterministic Resolved App Plan order.
pub trait CapabilityClientMany: CapabilityClient {
    /// Connects one typed client per bound provider without performing discovery.
    fn many_from_dependencies(
        dependencies: &Self::Dependencies,
    ) -> Result<Vec<BoundCapabilityClient<Self>>, Self::Error>;

    /// Connects every provider bound to one source-named requirement.
    fn many_from_requirement(
        dependencies: &Self::Dependencies,
        _requirement_id: &str,
    ) -> Result<Vec<BoundCapabilityClient<Self>>, Self::Error> {
        Self::many_from_dependencies(dependencies)
    }
}

/// One typed Capability client paired with its App-local provider Instance key.
#[derive(Debug)]
pub struct BoundCapabilityClient<C> {
    provider_instance: String,
    client: C,
}

impl<C> BoundCapabilityClient<C> {
    /// Creates one Plan-bound client entry.
    #[must_use]
    pub fn new(provider_instance: impl Into<String>, client: C) -> Self {
        Self {
            provider_instance: provider_instance.into(),
            client,
        }
    }

    /// Returns the App-local provider Instance key selected by Composition.
    #[must_use]
    pub fn provider_instance(&self) -> &str {
        &self.provider_instance
    }

    /// Returns the generated typed client.
    #[must_use]
    pub const fn client(&self) -> &C {
        &self.client
    }
}

impl<C> Deref for BoundCapabilityClient<C> {
    type Target = C;

    fn deref(&self) -> &Self::Target {
        &self.client
    }
}

/// A typed, lifecycle-bound Capability requirement declared by a Plugin.
///
/// Generated Plugin glue connects the Port during activation. Plugin behavior
/// can then call the generated Capability client directly through `Deref`.
/// A fresh Plugin generation owns fresh Ports; reconnecting one Port is an
/// invalid lifecycle transition.
pub struct Port<C: CapabilityClient> {
    client: Rc<OnceCell<C>>,
}

impl<C: CapabilityClient> Port<C> {
    /// Creates a disconnected typed Port.
    #[must_use]
    pub fn new() -> Self {
        Self {
            client: Rc::new(OnceCell::new()),
        }
    }

    /// Connects the Port from this Plugin Instance's resolved dependencies.
    pub fn connect(&self, dependencies: &C::Dependencies) -> Result<(), C::Error> {
        let client = C::from_dependencies(dependencies)?;
        self.client.set(client).map_err(|_| C::already_connected())
    }

    /// Connects the Port through one exact source declaration.
    pub fn connect_requirement(
        &self,
        dependencies: &C::Dependencies,
        requirement_id: &str,
    ) -> Result<(), C::Error> {
        let client = C::from_requirement(dependencies, requirement_id)?;
        self.client.set(client).map_err(|_| C::already_connected())
    }

    /// Returns whether lifecycle activation connected this Port.
    #[must_use]
    pub fn is_connected(&self) -> bool {
        self.client.get().is_some()
    }
}

impl<C: CapabilityClient> Clone for Port<C> {
    fn clone(&self) -> Self {
        Self {
            client: Rc::clone(&self.client),
        }
    }
}

impl<C: CapabilityClient> std::fmt::Debug for Port<C> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Port")
            .field("capability_id", &C::CAPABILITY_ID)
            .field("descriptor_version", &C::DESCRIPTOR_VERSION)
            .field("connected", &self.is_connected())
            .finish()
    }
}

impl<C: CapabilityClient> Default for Port<C> {
    fn default() -> Self {
        Self::new()
    }
}

impl<C: CapabilityClient> Deref for Port<C> {
    type Target = C;

    fn deref(&self) -> &Self::Target {
        self.client.get().unwrap_or_else(|| {
            panic!(
                "Capability Port {} was used before Plugin activation",
                C::CAPABILITY_ID
            )
        })
    }
}

/// A typed, lifecycle-bound `many` Capability requirement declared by a Plugin.
///
/// Generated Plugin glue connects one client per explicitly bound provider during
/// activation. Entries retain their provider Instance keys and resolved order.
pub struct ManyPort<C: CapabilityClientMany> {
    clients: Rc<OnceCell<Vec<BoundCapabilityClient<C>>>>,
}

impl<C: CapabilityClientMany> ManyPort<C> {
    /// Creates a disconnected typed `many` Port.
    #[must_use]
    pub fn new() -> Self {
        Self {
            clients: Rc::new(OnceCell::new()),
        }
    }

    /// Connects the Port from this Plugin Instance's resolved dependencies.
    pub fn connect(&self, dependencies: &C::Dependencies) -> Result<(), C::Error> {
        let clients = C::many_from_dependencies(dependencies)?;
        self.clients
            .set(clients)
            .map_err(|_| C::already_connected())
    }

    /// Connects only providers selected for one exact source declaration.
    pub fn connect_requirement(
        &self,
        dependencies: &C::Dependencies,
        requirement_id: &str,
    ) -> Result<(), C::Error> {
        let clients = C::many_from_requirement(dependencies, requirement_id)?;
        self.clients
            .set(clients)
            .map_err(|_| C::already_connected())
    }

    /// Returns whether lifecycle activation connected this Port.
    #[must_use]
    pub fn is_connected(&self) -> bool {
        self.clients.get().is_some()
    }
}

impl<C: CapabilityClientMany> Clone for ManyPort<C> {
    fn clone(&self) -> Self {
        Self {
            clients: Rc::clone(&self.clients),
        }
    }
}

impl<C: CapabilityClientMany> std::fmt::Debug for ManyPort<C> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ManyPort")
            .field("capability_id", &C::CAPABILITY_ID)
            .field("descriptor_version", &C::DESCRIPTOR_VERSION)
            .field("connected", &self.is_connected())
            .field("provider_count", &self.clients.get().map(Vec::len))
            .finish()
    }
}

impl<C: CapabilityClientMany> Default for ManyPort<C> {
    fn default() -> Self {
        Self::new()
    }
}

impl<C: CapabilityClientMany> Deref for ManyPort<C> {
    type Target = [BoundCapabilityClient<C>];

    fn deref(&self) -> &Self::Target {
        self.clients.get().map_or_else(
            || {
                panic!(
                    "Capability ManyPort {} was used before Plugin activation",
                    C::CAPABILITY_ID
                )
            },
            Vec::as_slice,
        )
    }
}

/// Common imports for a Plugin authoring frontend.
pub mod prelude {
    pub use crate::{
        BoundCapabilityClient, CapabilityClient, CapabilityClientMany, CapabilityReference,
        ManyPort, PluginError, Port,
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Eq, PartialEq)]
    struct ExampleClient(u64);

    #[derive(Debug, Eq, PartialEq)]
    enum ExampleError {
        AlreadyConnected,
    }

    impl CapabilityClient for ExampleClient {
        type Dependencies = ();
        type Error = ExampleError;

        const CAPABILITY_ID: &'static str = "example.echo@1";
        const DESCRIPTOR_VERSION: &'static str = "1.0.0";

        fn from_dependencies(_dependencies: &Self::Dependencies) -> Result<Self, Self::Error> {
            Ok(Self(42))
        }

        fn already_connected() -> Self::Error {
            ExampleError::AlreadyConnected
        }
    }

    impl CapabilityClientMany for ExampleClient {
        fn many_from_dependencies(
            _dependencies: &Self::Dependencies,
        ) -> Result<Vec<BoundCapabilityClient<Self>>, Self::Error> {
            Ok(vec![
                BoundCapabilityClient::new("alpha", Self(1)),
                BoundCapabilityClient::new("beta", Self(2)),
            ])
        }
    }

    #[test]
    fn port_connects_once_and_is_shared_by_plugin_clones() {
        let port = Port::<ExampleClient>::new();
        let plugin_clone = port.clone();
        assert!(!port.is_connected());

        port.connect(&())
            .expect("the generated client should connect");

        assert!(plugin_clone.is_connected());
        assert_eq!(plugin_clone.0, 42);
        assert_eq!(port.connect(&()), Err(ExampleError::AlreadyConnected));
    }

    #[test]
    fn many_port_preserves_provider_identity_and_resolved_order() {
        let port = ManyPort::<ExampleClient>::new();
        let plugin_clone = port.clone();
        assert!(!port.is_connected());

        port.connect(&())
            .expect("the generated clients should connect");

        assert!(plugin_clone.is_connected());
        assert_eq!(plugin_clone[0].provider_instance(), "alpha");
        assert_eq!(plugin_clone[0].client().0, 1);
        assert_eq!(plugin_clone[1].provider_instance(), "beta");
        assert_eq!(plugin_clone[1].client().0, 2);
        assert_eq!(port.connect(&()), Err(ExampleError::AlreadyConnected));
    }

    #[test]
    fn plugin_error_preserves_runtime_failures_while_mapping_domain_errors() {
        let domain = PluginError::<_, &str>::domain("missing").map_domain(str::len);
        assert_eq!(domain, PluginError::Domain(7));

        let runtime = PluginError::<&str, _>::runtime("cancelled").map_domain(str::len);
        assert_eq!(runtime, PluginError::Runtime("cancelled"));
    }

    #[derive(Debug, Eq, PartialEq)]
    struct NamedClient(String);

    impl CapabilityClient for NamedClient {
        type Dependencies = ();
        type Error = ExampleError;

        const CAPABILITY_ID: &'static str = "example.store@1";
        const DESCRIPTOR_VERSION: &'static str = "1.0.0";

        fn from_dependencies(_dependencies: &Self::Dependencies) -> Result<Self, Self::Error> {
            Ok(Self("legacy".to_owned()))
        }

        fn from_requirement(
            _dependencies: &Self::Dependencies,
            requirement_id: &str,
        ) -> Result<Self, Self::Error> {
            Ok(Self(requirement_id.to_owned()))
        }

        fn already_connected() -> Self::Error {
            ExampleError::AlreadyConnected
        }
    }

    impl CapabilityClientMany for NamedClient {
        fn many_from_dependencies(
            _dependencies: &Self::Dependencies,
        ) -> Result<Vec<BoundCapabilityClient<Self>>, Self::Error> {
            Ok(Vec::new())
        }

        fn many_from_requirement(
            _dependencies: &Self::Dependencies,
            requirement_id: &str,
        ) -> Result<Vec<BoundCapabilityClient<Self>>, Self::Error> {
            Ok(vec![BoundCapabilityClient::new(
                format!("{requirement_id}-provider"),
                Self(requirement_id.to_owned()),
            )])
        }
    }

    #[test]
    fn named_ports_pass_the_exact_source_requirement_to_generated_clients() {
        let source = Port::<NamedClient>::new();
        let destination = Port::<NamedClient>::new();
        source.connect_requirement(&(), "source").unwrap();
        destination.connect_requirement(&(), "destination").unwrap();
        assert_eq!(source.0, "source");
        assert_eq!(destination.0, "destination");

        let stores = ManyPort::<NamedClient>::new();
        stores.connect_requirement(&(), "stores").unwrap();
        assert_eq!(stores[0].provider_instance(), "stores-provider");
        assert_eq!(stores[0].0, "stores");
    }

    #[test]
    fn generated_capability_reference_is_typed_and_provider_free() {
        let reference = CapabilityReference::<NamedClient>::new(
            "example.store@1",
            "1.0.0",
            "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        );
        assert_eq!(reference.capability_id(), "example.store@1");
        assert_eq!(reference.descriptor_version(), "1.0.0");
        assert_eq!(
            reference.descriptor_digest(),
            "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        );
    }
}
