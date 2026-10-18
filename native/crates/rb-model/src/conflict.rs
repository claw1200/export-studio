//! Versioned inspection and decision values shared by independent frontends.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConflictKind {
    BaselineUnknown,
    MetadataChanged,
    PlaylistChanged,
    DeviceDeletion,
    DeviceOnlyContent,
    CueGridChanged,
    CrossFormatDivergence,
    AssetUnavailable,
    AssetChanged,
    OwnershipMismatch,
    IdentityChanged,
    RecoveryRequired,
    UnsupportedSchema,
    IntegrityFailure,
    HistoryReference,
    ExportPreflight,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Information,
    DecisionRequired,
    Blocked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntityKind {
    Track,
    Playlist,
    Folder,
    Tag,
    Device,
    Library,
    Export,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Resolution {
    UseLibrary,
    RestoreFromLibrary,
    DiscardDeviceEdits,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Diagnostic {
    pub id: String,
    pub kind: ConflictKind,
    pub severity: Severity,
    pub entity: EntityKind,
    pub entity_id: String,
    pub name: String,
    pub formats: Vec<String>,
    pub fields: Vec<String>,
    pub evidence: Vec<FieldEvidence>,
    pub detail: String,
    pub allowed_resolutions: Vec<Resolution>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldEvidence {
    pub field: String,
    pub baseline: Option<String>,
    pub device: Option<String>,
    pub library: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanSummary {
    pub tracks: usize,
    pub playlists: usize,
    pub estimated_peak_bytes: Option<u64>,
    pub approximate: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Inspection {
    pub token: String,
    pub diagnostics: Vec<Diagnostic>,
    pub plan: Option<PlanSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Decisions {
    pub token: String,
    pub resolutions: BTreeMap<String, Resolution>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decision_contract_roundtrips_with_stable_wire_names() {
        let decisions = Decisions {
            token: "revision-token".into(),
            resolutions: [("track:7:metadata".into(), Resolution::UseLibrary)].into(),
        };
        let value = serde_json::to_value(&decisions).unwrap();
        assert_eq!(value["resolutions"]["track:7:metadata"], "use_library");
        assert_eq!(serde_json::from_value::<Decisions>(value).unwrap(), decisions);
    }

    #[test]
    fn unsupported_force_action_is_not_in_the_resolution_contract() {
        assert!(serde_json::from_str::<Resolution>("\"force\"").is_err());
    }
}