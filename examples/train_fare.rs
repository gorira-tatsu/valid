#[path = "support/fare.rs"]
mod fare;

use fare::{
    calculate_fare, collect_fare_coverage, explain_fare, verify_child_never_costs_more_than_adult,
    verify_day_pass_is_zero, verify_longer_distance_is_not_cheaper, FareRequest, RiderCategory,
    StationZone, TicketKind, TransferWindow,
};

fn main() {
    let request = FareRequest {
        origin: StationZone::Zone1,
        destination: StationZone::Zone3,
        rider: RiderCategory::Child,
        ticket: TicketKind::SingleRide,
        transfer: TransferWindow::Within90Minutes,
    };

    let decision = calculate_fare(request);
    let summary = explain_fare(request);
    let coverage = collect_fare_coverage();

    println!("fare: {} yen", decision.total_yen);
    println!("rules: {:?}", decision.applied_rules);
    println!("summary: {}", summary);
    println!(
        "invariants: child<=adult={} daypass_zero={} longer_not_cheaper={}",
        verify_child_never_costs_more_than_adult().is_empty(),
        verify_day_pass_is_zero().is_empty(),
        verify_longer_distance_is_not_cheaper().is_empty()
    );
    println!("coverage total_requests: {}", coverage.total_requests);
    println!("coverage rule_counts: {:?}", coverage.rule_counts);
}
