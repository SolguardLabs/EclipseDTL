use eclipsedtl::{
    AccountId, Amount, AssetId, CapitalBand, CapitalPolicy, NetworkCapitalReport, OperatorBook,
    OperatorCapitalSnapshot, OperatorId, OperatorProfile, RouteId,
};

fn operator(id: &str, pledged: u128) -> OperatorProfile {
    let mut profile = OperatorProfile::new(
        OperatorId::new(id),
        format!("{id} desk"),
        "primary",
        AccountId::new(format!("fee-{id}")),
    );
    profile.pledge(Amount(pledged)).unwrap();
    profile
}

fn attach(profile: &mut OperatorProfile, route: &str, amount: u128) {
    profile
        .attach_guarantee(
            RouteId::new(route),
            AssetId::new("ELIQ"),
            Amount(amount),
            format!("batch-{route}"),
        )
        .unwrap();
}

#[test]
fn healthy_operator_has_surplus_after_stress() {
    let mut profile = operator("alpha", 10_000);
    attach(&mut profile, "route-a", 2_000);
    let snapshot = OperatorCapitalSnapshot::assess(&profile, &CapitalPolicy::default()).unwrap();

    assert_eq!(snapshot.band, CapitalBand::Healthy);
    assert_eq!(snapshot.stressed_exposure, 2_100);
    assert_eq!(snapshot.effective_guarantee, 9_800);
    assert_eq!(snapshot.surplus, 7_700);
    assert_eq!(snapshot.shortfall, 0);
}

#[test]
fn coverage_warning_is_reported_before_minimum_is_crossed() {
    let mut profile = operator("alpha", 10_000);
    attach(&mut profile, "route-a", 8_000);
    let snapshot = OperatorCapitalSnapshot::assess(&profile, &CapitalPolicy::default()).unwrap();

    assert_eq!(snapshot.band, CapitalBand::Watch);
    assert!(snapshot.coverage_bps >= 10_000);
    assert!(snapshot.coverage_bps < 12_000);
}

#[test]
fn constrained_band_combines_coverage_and_utilization() {
    let mut profile = operator("alpha", 10_000);
    attach(&mut profile, "route-a", 9_000);
    let snapshot = OperatorCapitalSnapshot::assess(&profile, &CapitalPolicy::default()).unwrap();

    assert_eq!(snapshot.band, CapitalBand::Constrained);
    assert_eq!(snapshot.utilization_bps, 9_000);
    assert!(snapshot.shortfall > 0);
}

#[test]
fn exhausted_band_requires_exposure_without_effective_capital() {
    let mut profile = operator("alpha", 5_000);
    attach(&mut profile, "route-a", 5_000);
    profile.guarantee.slash(Amount(5_000)).unwrap();
    let snapshot = OperatorCapitalSnapshot::assess(&profile, &CapitalPolicy::default()).unwrap();

    assert_eq!(snapshot.band, CapitalBand::Exhausted);
    assert_eq!(snapshot.effective_guarantee, 0);
    assert_eq!(snapshot.shortfall, 5_250);
}

#[test]
fn empty_exposure_keeps_full_capacity_without_division_errors() {
    let profile = operator("alpha", 5_000);
    let snapshot = OperatorCapitalSnapshot::assess(&profile, &CapitalPolicy::default()).unwrap();

    assert_eq!(snapshot.band, CapitalBand::Healthy);
    assert_eq!(snapshot.coverage_bps, 1_000_000);
    assert_eq!(snapshot.concentration_hhi_bps, 0);
}

#[test]
fn a_single_exposure_bucket_has_maximum_concentration() {
    let mut profile = operator("alpha", 10_000);
    attach(&mut profile, "route-a", 2_000);
    let snapshot = OperatorCapitalSnapshot::assess(&profile, &CapitalPolicy::default()).unwrap();

    assert_eq!(snapshot.largest_bucket_share_bps, 10_000);
    assert_eq!(snapshot.concentration_hhi_bps, 10_000);
}

#[test]
fn diversification_reduces_hhi_and_largest_share() {
    let mut profile = operator("alpha", 12_000);
    attach(&mut profile, "route-a", 2_000);
    attach(&mut profile, "route-b", 2_000);
    profile.allocate_external_commitment(Amount(2_000)).unwrap();
    let snapshot = OperatorCapitalSnapshot::assess(&profile, &CapitalPolicy::default()).unwrap();

    assert_eq!(snapshot.recorded_exposure, 6_000);
    assert_eq!(snapshot.largest_bucket_share_bps, 3_333);
    assert_eq!(snapshot.concentration_hhi_bps, 3_330);
}

#[test]
fn exposure_addon_changes_stress_capacity_deterministically() {
    let mut profile = operator("alpha", 10_000);
    attach(&mut profile, "route-a", 4_000);
    let base = OperatorCapitalSnapshot::assess(&profile, &CapitalPolicy::default()).unwrap();
    let policy = CapitalPolicy {
        exposure_addon_bps: 2_000,
        ..CapitalPolicy::default()
    };
    let stressed = OperatorCapitalSnapshot::assess(&profile, &policy).unwrap();

    assert_eq!(base.stressed_exposure, 4_200);
    assert_eq!(stressed.stressed_exposure, 4_800);
    assert!(stressed.coverage_bps < base.coverage_bps);
}

#[test]
fn network_report_aggregates_capital_and_operator_concentration() {
    let mut book = OperatorBook::new();
    let mut alpha = operator("alpha", 10_000);
    let mut beta = operator("beta", 8_000);
    attach(&mut alpha, "route-a", 4_000);
    attach(&mut beta, "route-b", 2_000);
    book.insert(alpha).unwrap();
    book.insert(beta).unwrap();

    let report = NetworkCapitalReport::assess(&book, &CapitalPolicy::default()).unwrap();

    assert_eq!(report.operators.len(), 2);
    assert_eq!(report.total_pledged, 18_000);
    assert_eq!(report.total_recorded_exposure, 6_000);
    assert_eq!(report.total_stressed_exposure, 6_300);
    assert_eq!(report.healthy_operators, 2);
    assert_eq!(report.largest_operator_share_bps, 6_666);
    assert_eq!(report.operator_concentration_hhi_bps, 5_553);
}

#[test]
fn invalid_policy_fails_closed() {
    let profile = operator("alpha", 10_000);
    let invalid = CapitalPolicy {
        minimum_coverage_bps: 12_000,
        warning_coverage_bps: 11_000,
        ..CapitalPolicy::default()
    };

    let error = OperatorCapitalSnapshot::assess(&profile, &invalid).unwrap_err();
    assert_eq!(error.code(), "INVALID_SCENARIO");
}
