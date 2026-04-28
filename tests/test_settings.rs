//! Tests for the pure logic in `components::settings` — persona mode,
//! complexity tier derivation, and feature visibility helpers.

use coreml_playground::components::settings::{
    should_show_batch, should_show_introspection, should_show_raw_json, should_show_sidebar,
    ComplexityTier, PersonaMode,
};

// ---------------------------------------------------------------------------
// ComplexityTier::from_usage
// ---------------------------------------------------------------------------

#[test]
fn tier_from_usage_zero_zero_is_tier1() {
    assert_eq!(ComplexityTier::from_usage(0, 0), ComplexityTier::Tier1);
}

#[test]
fn tier_from_usage_below_thresholds_is_tier1() {
    // 4 sessions, 19 inferences — just below both thresholds.
    assert_eq!(ComplexityTier::from_usage(4, 19), ComplexityTier::Tier1);
}

#[test]
fn tier_from_usage_five_sessions_is_tier2() {
    assert_eq!(ComplexityTier::from_usage(5, 0), ComplexityTier::Tier2);
}

#[test]
fn tier_from_usage_twenty_inferences_is_tier3() {
    assert_eq!(ComplexityTier::from_usage(0, 20), ComplexityTier::Tier3);
}

#[test]
fn tier_from_usage_both_high_is_tier3() {
    // inference_count >= 20 takes priority over session_count.
    assert_eq!(ComplexityTier::from_usage(100, 100), ComplexityTier::Tier3);
}

#[test]
fn tier_from_usage_many_sessions_few_inferences_is_tier2() {
    assert_eq!(ComplexityTier::from_usage(50, 10), ComplexityTier::Tier2);
}

// ---------------------------------------------------------------------------
// should_show_sidebar — all 9 persona x tier combos
// ---------------------------------------------------------------------------

#[test]
fn sidebar_explorer_tier1_hidden() {
    assert!(!should_show_sidebar(
        PersonaMode::Explorer,
        ComplexityTier::Tier1
    ));
}

#[test]
fn sidebar_explorer_tier2_shown() {
    assert!(should_show_sidebar(
        PersonaMode::Explorer,
        ComplexityTier::Tier2
    ));
}

#[test]
fn sidebar_explorer_tier3_shown() {
    assert!(should_show_sidebar(
        PersonaMode::Explorer,
        ComplexityTier::Tier3
    ));
}

#[test]
fn sidebar_developer_always_shown() {
    assert!(should_show_sidebar(
        PersonaMode::Developer,
        ComplexityTier::Tier1
    ));
    assert!(should_show_sidebar(
        PersonaMode::Developer,
        ComplexityTier::Tier2
    ));
    assert!(should_show_sidebar(
        PersonaMode::Developer,
        ComplexityTier::Tier3
    ));
}

#[test]
fn sidebar_researcher_always_shown() {
    assert!(should_show_sidebar(
        PersonaMode::Researcher,
        ComplexityTier::Tier1
    ));
    assert!(should_show_sidebar(
        PersonaMode::Researcher,
        ComplexityTier::Tier2
    ));
    assert!(should_show_sidebar(
        PersonaMode::Researcher,
        ComplexityTier::Tier3
    ));
}

// ---------------------------------------------------------------------------
// should_show_introspection — all 9 combos
// ---------------------------------------------------------------------------

#[test]
fn introspection_explorer_tier1_hidden() {
    assert!(!should_show_introspection(
        PersonaMode::Explorer,
        ComplexityTier::Tier1
    ));
}

#[test]
fn introspection_explorer_tier2_shown() {
    assert!(should_show_introspection(
        PersonaMode::Explorer,
        ComplexityTier::Tier2
    ));
}

#[test]
fn introspection_explorer_tier3_shown() {
    assert!(should_show_introspection(
        PersonaMode::Explorer,
        ComplexityTier::Tier3
    ));
}

#[test]
fn introspection_developer_always_shown() {
    assert!(should_show_introspection(
        PersonaMode::Developer,
        ComplexityTier::Tier1
    ));
    assert!(should_show_introspection(
        PersonaMode::Developer,
        ComplexityTier::Tier2
    ));
    assert!(should_show_introspection(
        PersonaMode::Developer,
        ComplexityTier::Tier3
    ));
}

#[test]
fn introspection_researcher_always_shown() {
    assert!(should_show_introspection(
        PersonaMode::Researcher,
        ComplexityTier::Tier1
    ));
    assert!(should_show_introspection(
        PersonaMode::Researcher,
        ComplexityTier::Tier2
    ));
    assert!(should_show_introspection(
        PersonaMode::Researcher,
        ComplexityTier::Tier3
    ));
}

// ---------------------------------------------------------------------------
// should_show_batch — 3 personas
// ---------------------------------------------------------------------------

#[test]
fn batch_only_for_researcher() {
    assert!(!should_show_batch(PersonaMode::Explorer));
    assert!(!should_show_batch(PersonaMode::Developer));
    assert!(should_show_batch(PersonaMode::Researcher));
}

// ---------------------------------------------------------------------------
// should_show_raw_json — 3 personas
// ---------------------------------------------------------------------------

#[test]
fn raw_json_only_for_researcher() {
    assert!(!should_show_raw_json(PersonaMode::Explorer));
    assert!(!should_show_raw_json(PersonaMode::Developer));
    assert!(should_show_raw_json(PersonaMode::Researcher));
}
