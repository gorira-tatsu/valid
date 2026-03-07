#![allow(dead_code)]

//! Train fare verification as an example Rust model.

use std::collections::BTreeMap;

use valid::modeling::Finite;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum StationZone {
    Zone1,
    Zone2,
    Zone3,
}

impl Finite for StationZone {
    fn all() -> Vec<Self> {
        vec![Self::Zone1, Self::Zone2, Self::Zone3]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum RiderCategory {
    Adult,
    Child,
}

impl Finite for RiderCategory {
    fn all() -> Vec<Self> {
        vec![Self::Adult, Self::Child]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum TicketKind {
    SingleRide,
    DayPass,
}

impl Finite for TicketKind {
    fn all() -> Vec<Self> {
        vec![Self::SingleRide, Self::DayPass]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum TransferWindow {
    None,
    Within90Minutes,
    Expired,
}

impl Finite for TransferWindow {
    fn all() -> Vec<Self> {
        vec![Self::None, Self::Within90Minutes, Self::Expired]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FareRequest {
    pub origin: StationZone,
    pub destination: StationZone,
    pub rider: RiderCategory,
    pub ticket: TicketKind,
    pub transfer: TransferWindow,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FareDecision {
    pub total_yen: u16,
    pub applied_rules: Vec<&'static str>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FareCoverageReport {
    pub total_requests: usize,
    pub rule_counts: BTreeMap<&'static str, usize>,
    pub max_fare: u16,
    pub min_fare: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FareViolation {
    pub message: String,
    pub request: FareRequest,
    pub decision: FareDecision,
}

fn zone_distance(origin: StationZone, destination: StationZone) -> u16 {
    use StationZone::{Zone1, Zone2, Zone3};
    match (origin, destination) {
        (Zone1, Zone1) | (Zone2, Zone2) | (Zone3, Zone3) => 0,
        (Zone1, Zone2) | (Zone2, Zone1) | (Zone2, Zone3) | (Zone3, Zone2) => 1,
        (Zone1, Zone3) | (Zone3, Zone1) => 2,
    }
}

fn adult_base_fare(distance: u16) -> u16 {
    match distance {
        0 => 140,
        1 => 220,
        _ => 310,
    }
}

fn child_fare(adult: u16) -> u16 {
    let half = adult / 2;
    ((half + 5) / 10) * 10
}

pub fn enumerate_requests() -> Vec<FareRequest> {
    let mut requests = Vec::new();
    for origin in StationZone::all() {
        for destination in StationZone::all() {
            for rider in RiderCategory::all() {
                for ticket in TicketKind::all() {
                    for transfer in TransferWindow::all() {
                        requests.push(FareRequest {
                            origin,
                            destination,
                            rider,
                            ticket,
                            transfer,
                        });
                    }
                }
            }
        }
    }
    requests
}

pub fn calculate_fare(request: FareRequest) -> FareDecision {
    if matches!(request.ticket, TicketKind::DayPass) {
        return FareDecision {
            total_yen: 0,
            applied_rules: vec!["day_pass_zero"],
        };
    }

    let distance = zone_distance(request.origin, request.destination);
    let mut applied_rules = Vec::new();
    let mut total = adult_base_fare(distance);
    applied_rules.push(match distance {
        0 => "same_zone_base",
        1 => "adjacent_zone_base",
        _ => "long_distance_base",
    });

    if matches!(request.rider, RiderCategory::Child) {
        total = child_fare(total);
        applied_rules.push("child_discount");
    }

    if matches!(request.transfer, TransferWindow::Within90Minutes) {
        total = total.saturating_sub(20).max(100);
        applied_rules.push("transfer_discount");
    }

    FareDecision {
        total_yen: total,
        applied_rules,
    }
}

pub fn explain_fare(request: FareRequest) -> String {
    let decision = calculate_fare(request);
    format!(
        "fare={} rules=[{}]",
        decision.total_yen,
        decision.applied_rules.join(", ")
    )
}

pub fn collect_fare_coverage() -> FareCoverageReport {
    let requests = enumerate_requests();
    let mut rule_counts = BTreeMap::new();
    let mut max_fare = 0u16;
    let mut min_fare = u16::MAX;

    for request in requests.iter().copied() {
        let decision = calculate_fare(request);
        max_fare = max_fare.max(decision.total_yen);
        min_fare = min_fare.min(decision.total_yen);
        for rule in decision.applied_rules {
            *rule_counts.entry(rule).or_insert(0) += 1;
        }
    }

    FareCoverageReport {
        total_requests: requests.len(),
        rule_counts,
        max_fare,
        min_fare,
    }
}

pub fn verify_child_never_costs_more_than_adult() -> Vec<FareViolation> {
    let mut violations = Vec::new();
    for origin in StationZone::all() {
        for destination in StationZone::all() {
            for ticket in TicketKind::all() {
                for transfer in TransferWindow::all() {
                    let adult_request = FareRequest {
                        origin,
                        destination,
                        rider: RiderCategory::Adult,
                        ticket,
                        transfer,
                    };
                    let child_request = FareRequest {
                        rider: RiderCategory::Child,
                        ..adult_request
                    };
                    let adult = calculate_fare(adult_request);
                    let child = calculate_fare(child_request);
                    if child.total_yen > adult.total_yen {
                        violations.push(FareViolation {
                            message: "child fare exceeds adult fare".to_string(),
                            request: child_request,
                            decision: child,
                        });
                    }
                }
            }
        }
    }
    violations
}

pub fn verify_day_pass_is_zero() -> Vec<FareViolation> {
    enumerate_requests()
        .into_iter()
        .filter(|request| matches!(request.ticket, TicketKind::DayPass))
        .filter_map(|request| {
            let decision = calculate_fare(request);
            if decision.total_yen == 0 {
                None
            } else {
                Some(FareViolation {
                    message: "day pass request should be free".to_string(),
                    request,
                    decision,
                })
            }
        })
        .collect()
}

pub fn verify_longer_distance_is_not_cheaper() -> Vec<FareViolation> {
    let mut violations = Vec::new();
    for rider in RiderCategory::all() {
        for ticket in TicketKind::all() {
            for transfer in TransferWindow::all() {
                let near = calculate_fare(FareRequest {
                    origin: StationZone::Zone1,
                    destination: StationZone::Zone2,
                    rider,
                    ticket,
                    transfer,
                });
                let far = calculate_fare(FareRequest {
                    origin: StationZone::Zone1,
                    destination: StationZone::Zone3,
                    rider,
                    ticket,
                    transfer,
                });
                if far.total_yen < near.total_yen {
                    violations.push(FareViolation {
                        message: "longer journey became cheaper than adjacent-zone journey"
                            .to_string(),
                        request: FareRequest {
                            origin: StationZone::Zone1,
                            destination: StationZone::Zone3,
                            rider,
                            ticket,
                            transfer,
                        },
                        decision: far,
                    });
                }
            }
        }
    }
    violations
}

#[cfg(test)]
mod tests {
    use super::{
        calculate_fare, collect_fare_coverage, explain_fare,
        verify_child_never_costs_more_than_adult, verify_day_pass_is_zero,
        verify_longer_distance_is_not_cheaper, FareRequest, RiderCategory, StationZone, TicketKind,
        TransferWindow,
    };

    #[test]
    fn day_pass_is_zero() {
        assert!(verify_day_pass_is_zero().is_empty());
    }

    #[test]
    fn child_never_costs_more_than_adult() {
        assert!(verify_child_never_costs_more_than_adult().is_empty());
    }

    #[test]
    fn longer_journey_is_not_cheaper() {
        assert!(verify_longer_distance_is_not_cheaper().is_empty());
    }

    #[test]
    fn explanation_mentions_applied_rules() {
        let summary = explain_fare(FareRequest {
            origin: StationZone::Zone1,
            destination: StationZone::Zone3,
            rider: RiderCategory::Child,
            ticket: TicketKind::SingleRide,
            transfer: TransferWindow::Within90Minutes,
        });
        assert!(summary.contains("long_distance_base"));
        assert!(summary.contains("child_discount"));
        assert!(summary.contains("transfer_discount"));
    }

    #[test]
    fn coverage_reports_core_rules() {
        let coverage = collect_fare_coverage();
        assert!(coverage.total_requests > 0);
        assert_eq!(coverage.min_fare, 0);
        assert!(coverage.max_fare >= 290);
        assert!(coverage.rule_counts.contains_key("day_pass_zero"));
        assert!(coverage.rule_counts.contains_key("transfer_discount"));
    }

    #[test]
    fn transfer_discount_reduces_single_ride_price() {
        let without_transfer = calculate_fare(FareRequest {
            origin: StationZone::Zone1,
            destination: StationZone::Zone2,
            rider: RiderCategory::Adult,
            ticket: TicketKind::SingleRide,
            transfer: TransferWindow::None,
        });
        let with_transfer = calculate_fare(FareRequest {
            transfer: TransferWindow::Within90Minutes,
            ..FareRequest {
                origin: StationZone::Zone1,
                destination: StationZone::Zone2,
                rider: RiderCategory::Adult,
                ticket: TicketKind::SingleRide,
                transfer: TransferWindow::None,
            }
        });
        assert!(with_transfer.total_yen < without_transfer.total_yen);
    }
}
