//! Public facade for the Lenso backend framework.

#[cfg(any(feature = "host", feature = "host-transactions"))]
pub mod host;

/// Console UI and contribution declarations used by Module authors.
pub mod console {
    pub use lenso_contracts::{
        CONSOLE_BRIDGE_PROTOCOL, ConsoleActionInputBinding, ConsoleActionInputValue,
        ConsoleContribution, ConsoleContributionAction, ConsoleContributionKind, ConsoleNavigation,
        ConsoleNavigationGroup, ConsolePermissionGrant, ConsolePermissionRequest, ConsoleSlot,
        ConsoleSlotContext, ConsoleSlotContextField, ConsoleSlotContextFieldType, ConsoleSurface,
        ConsoleSurfacePresentation, ConsoleUiArtifact, ConsoleUiArtifactEntry,
        ConsoleUiArtifactFormat, ConsoleWorkspaceRef,
    };
}

/// Public System Plane wire contracts and verification primitives.
#[cfg(feature = "service")]
pub mod system_plane {
    pub use lenso_service::system_plane::*;
}

pub use lenso_contracts::*;
