//! Public facade for the Lenso backend framework.

#[cfg(any(
    feature = "host",
    feature = "host-transactions",
    feature = "linked-module"
))]
pub mod host;

/// Console UI and contribution declarations used by Module authors.
pub mod console {
    pub use lenso_contracts::{
        CONSOLE_BRIDGE_PROTOCOL, CONSOLE_MODULE_PROTOCOL, CONSOLE_MODULE_PROTOCOL_MAJOR,
        CONSOLE_UI_ESM_FORMAT, ConsoleActionInputBinding, ConsoleActionInputValue,
        ConsoleContribution, ConsoleContributionAction, ConsoleContributionKind,
        ConsoleModuleManifest, ConsoleModuleSurface, ConsoleNavigation, ConsoleNavigationGroup,
        ConsolePermissionGrant, ConsolePermissionRequest, ConsoleSlot, ConsoleSlotContext,
        ConsoleSlotContextField, ConsoleSlotContextFieldType, ConsoleSurface, ConsoleSurfaceArea,
        ConsoleSurfacePresentation, ConsoleUiArtifact, ConsoleUiArtifactEntry,
        ConsoleUiArtifactFormat, ConsoleUiArtifactStyleAsset, ConsoleWorkspaceRef,
    };
}

/// Public System Plane wire contracts and verification primitives.
#[cfg(feature = "service")]
pub mod system_plane {
    pub use lenso_service::system_plane::*;
}

/// Public Workload Control Adapter contracts.
#[cfg(feature = "service")]
pub mod workload_control {
    pub use lenso_service::workload_control::*;
}

pub use lenso_contracts::*;
