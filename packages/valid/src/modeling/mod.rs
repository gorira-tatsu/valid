//! Rust-based modeling contracts.
//!
//! This module exposes only generic system-side contracts. Concrete domain
//! models belong in user code, examples, or tests rather than inside `src/`.

use std::{
    collections::{BTreeMap, BTreeSet},
    collections::{HashSet, VecDeque},
    fmt::Debug,
    hash::Hash,
    marker::PhantomData,
};

use crate::{
    api::{ExplainCandidateCause, ExplainResponse},
    contract::snapshot_model,
    coverage::CoverageReport,
    engine::{
        AssuranceLevel, BackendKind, CheckOutcome, ExplicitRunResult, PropertyResult,
        PropertySelection, RunManifest, RunPlan, RunStatus,
    },
    evidence::{EvidenceKind, EvidenceTrace, TraceStep},
    ir::{
        ActionIr, BinaryOp, ExprIr, FieldType, InitAssignment, ModelIr, PropertyIr, SourceSpan,
        StateField, UnaryOp, UpdateIr, Value,
    },
    solver::{run_with_adapter, AdapterConfig},
    support::hash::stable_hash_hex,
    testgen::{build_counterexample_vector, TestVector, VectorActionStep},
};

pub trait IntoModelValue {
    fn into_model_value(self) -> Value;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FiniteEnumSet<T> {
    bits: u64,
    _marker: PhantomData<T>,
}

impl<T> FiniteEnumSet<T>
where
    T: FiniteValueSpec,
{
    pub fn empty() -> Self {
        Self {
            bits: 0,
            _marker: PhantomData,
        }
    }

    pub fn from_items(items: &[T]) -> Self {
        let mut set = Self::empty();
        for item in items {
            set = set.insert(item.clone());
        }
        set
    }

    pub fn bits(self) -> u64 {
        self.bits
    }

    pub fn contains(self, value: T) -> bool {
        let mask = enum_variant_mask(value.variant_index());
        self.bits & mask != 0
    }

    pub fn insert(self, value: T) -> Self {
        let mask = enum_variant_mask(value.variant_index());
        Self {
            bits: self.bits | mask,
            _marker: PhantomData,
        }
    }

    pub fn remove(self, value: T) -> Self {
        let mask = enum_variant_mask(value.variant_index());
        Self {
            bits: self.bits & !mask,
            _marker: PhantomData,
        }
    }

    pub fn is_empty(self) -> bool {
        self.bits == 0
    }
}

impl<T> Default for FiniteEnumSet<T>
where
    T: FiniteValueSpec,
{
    fn default() -> Self {
        Self::empty()
    }
}

impl IntoModelValue for bool {
    fn into_model_value(self) -> Value {
        Value::Bool(self)
    }
}

impl IntoModelValue for u8 {
    fn into_model_value(self) -> Value {
        Value::UInt(self as u64)
    }
}

impl IntoModelValue for u16 {
    fn into_model_value(self) -> Value {
        Value::UInt(self as u64)
    }
}

impl IntoModelValue for u32 {
    fn into_model_value(self) -> Value {
        Value::UInt(self as u64)
    }
}

impl IntoModelValue for u64 {
    fn into_model_value(self) -> Value {
        Value::UInt(self)
    }
}

pub trait FiniteValueSpec: Clone + Debug + Eq + Hash {
    fn variant_labels() -> &'static [&'static str];
    fn variant_index(&self) -> u64;
    fn variant_label(&self) -> &'static str;
}

pub trait FiniteSetSpec: Clone + Debug + Eq + Hash {
    fn variant_labels() -> &'static [&'static str];
}

impl<T> FiniteSetSpec for FiniteEnumSet<T>
where
    T: FiniteValueSpec,
{
    fn variant_labels() -> &'static [&'static str] {
        T::variant_labels()
    }
}

impl<T> FiniteValueSpec for Option<T>
where
    T: FiniteValueSpec,
{
    fn variant_labels() -> &'static [&'static str] {
        let mut labels = Vec::with_capacity(T::variant_labels().len() + 1);
        labels.push("None");
        labels.extend(
            T::variant_labels()
                .iter()
                .map(|label| Box::leak(format!("Some({label})").into_boxed_str()) as &'static str),
        );
        Box::leak(labels.into_boxed_slice())
    }

    fn variant_index(&self) -> u64 {
        match self {
            None => 0,
            Some(value) => value.variant_index() + 1,
        }
    }

    fn variant_label(&self) -> &'static str {
        match self {
            None => "None",
            Some(value) => Box::leak(format!("Some({})", value.variant_label()).into_boxed_str()),
        }
    }
}

impl<T> IntoModelValue for T
where
    T: FiniteValueSpec,
{
    fn into_model_value(self) -> Value {
        Value::EnumVariant {
            label: self.variant_label().to_string(),
            index: self.variant_index(),
        }
    }
}

impl<T> IntoModelValue for FiniteEnumSet<T>
where
    T: FiniteValueSpec,
{
    fn into_model_value(self) -> Value {
        Value::UInt(self.bits())
    }
}

pub fn implies(left: bool, right: bool) -> bool {
    !left || right
}

pub fn iff(left: bool, right: bool) -> bool {
    left == right
}

pub fn xor(left: bool, right: bool) -> bool {
    left ^ right
}

pub fn contains<T>(set: FiniteEnumSet<T>, value: T) -> bool
where
    T: FiniteValueSpec,
{
    set.contains(value)
}

pub fn insert<T>(set: FiniteEnumSet<T>, value: T) -> FiniteEnumSet<T>
where
    T: FiniteValueSpec,
{
    set.insert(value)
}

pub fn remove<T>(set: FiniteEnumSet<T>, value: T) -> FiniteEnumSet<T>
where
    T: FiniteValueSpec,
{
    set.remove(value)
}

pub fn is_empty<T>(set: FiniteEnumSet<T>) -> bool
where
    T: FiniteValueSpec,
{
    set.is_empty()
}

fn enum_variant_mask(index: u64) -> u64 {
    1u64.checked_shl(index as u32).unwrap_or(0)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelingRunStatus {
    Pass,
    Fail,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelingTraceStep<S, A> {
    pub index: usize,
    pub action: A,
    pub state_before: S,
    pub state_after: S,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelingCheckResult<S, A> {
    pub model_id: &'static str,
    pub property_id: &'static str,
    pub status: ModelingRunStatus,
    pub explored_states: usize,
    pub explored_transitions: usize,
    pub trace: Vec<ModelingTraceStep<S, A>>,
}

pub trait Finite: Sized {
    fn all() -> Vec<Self>;
}

pub trait ModelingState: Clone + Debug + Eq + Hash {
    fn snapshot(&self) -> BTreeMap<String, Value>;
}

pub trait ModelingAction: Clone + Debug + Eq + Hash + Finite {
    fn action_id(&self) -> String;

    fn action_label(&self) -> String {
        self.action_id()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateFieldDescriptor {
    pub name: &'static str,
    pub rust_type: &'static str,
    pub range: Option<&'static str>,
    pub variants: Option<Vec<&'static str>>,
    pub is_set: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionDescriptor {
    pub variant: &'static str,
    pub action_id: &'static str,
    pub reads: &'static [&'static str],
    pub writes: &'static [&'static str],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransitionDescriptor {
    pub action_variant: &'static str,
    pub action_id: &'static str,
    pub guard: &'static str,
    pub effect: &'static str,
    pub reads: &'static [&'static str],
    pub writes: &'static [&'static str],
    pub path_tags: &'static [&'static str],
    pub updates: &'static [TransitionUpdateDescriptor],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransitionUpdateDescriptor {
    pub field: &'static str,
    pub expr: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MachineTransitionUpdateIr {
    pub field: &'static str,
    pub expr: Option<&'static str>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MachineTransitionIr {
    pub action_variant: &'static str,
    pub action_id: &'static str,
    pub guard: Option<&'static str>,
    pub effect: Option<&'static str>,
    pub reads: &'static [&'static str],
    pub writes: &'static [&'static str],
    pub path_tags: Vec<&'static str>,
    pub updates: Vec<MachineTransitionUpdateIr>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MachineCapabilityReport {
    pub parse_ready: bool,
    pub explicit_ready: bool,
    pub ir_ready: bool,
    pub solver_ready: bool,
    pub coverage_ready: bool,
    pub explain_ready: bool,
    pub testgen_ready: bool,
    pub machine_ir_error: Option<String>,
    pub reasons: Vec<String>,
}

pub fn infer_decision_path_tags<'a, RI, WI>(
    action_id: &str,
    reads: RI,
    writes: WI,
    guard: Option<&str>,
    effect: Option<&str>,
) -> Vec<String>
where
    RI: IntoIterator<Item = &'a str>,
    WI: IntoIterator<Item = &'a str>,
{
    let reads = reads.into_iter().collect::<Vec<_>>();
    let writes = writes.into_iter().collect::<Vec<_>>();
    let mut tags = BTreeSet::new();
    if guard.is_some() {
        tags.insert("guard_path".to_string());
    }
    if !reads.is_empty() {
        tags.insert("read_path".to_string());
    }
    if !writes.is_empty() {
        tags.insert("write_path".to_string());
    }
    let mut text = action_id.to_ascii_lowercase();
    for part in &reads {
        text.push(' ');
        text.push_str(&part.to_ascii_lowercase());
    }
    for part in &writes {
        text.push(' ');
        text.push_str(&part.to_ascii_lowercase());
    }
    if let Some(guard) = guard {
        text.push(' ');
        text.push_str(&guard.to_ascii_lowercase());
    }
    if let Some(effect) = effect {
        text.push(' ');
        text.push_str(&effect.to_ascii_lowercase());
    }
    for (needle, tag) in [
        ("allow", "allow_path"),
        ("deny", "deny_path"),
        ("boundary", "boundary_path"),
        ("exception", "exception_path"),
        ("session", "session_path"),
        ("lock", "state_gate_path"),
    ] {
        if text.contains(needle) {
            tags.insert(tag.to_string());
        }
    }
    if tags.is_empty() {
        tags.insert("transition_path".to_string());
    }
    tags.into_iter().collect()
}

pub fn decision_path_tags<'a, RI, WI>(
    explicit_tags: &[&str],
    action_id: &str,
    reads: RI,
    writes: WI,
    guard: Option<&str>,
    effect: Option<&str>,
) -> Vec<String>
where
    RI: IntoIterator<Item = &'a str>,
    WI: IntoIterator<Item = &'a str>,
{
    let mut tags = explicit_tags
        .iter()
        .map(|tag| tag.to_string())
        .collect::<BTreeSet<_>>();
    tags.extend(infer_decision_path_tags(
        action_id, reads, writes, guard, effect,
    ));
    tags.into_iter().collect()
}

pub trait StateSpec: ModelingState {
    fn state_fields() -> Vec<StateFieldDescriptor>;
}

pub trait ActionSpec: ModelingAction {
    fn action_descriptors() -> Vec<ActionDescriptor>;
}

#[derive(Clone)]
pub struct ModelProperty<S> {
    pub property_id: &'static str,
    pub property_kind: crate::ir::PropertyKind,
    pub expr: Option<&'static str>,
    pub holds: fn(&S) -> bool,
}

impl<S> ModelProperty<S> {
    pub fn invariant(property_id: &'static str, holds: fn(&S) -> bool) -> Self {
        Self::invariant_expr(property_id, None, holds)
    }

    pub fn invariant_expr(
        property_id: &'static str,
        expr: Option<&'static str>,
        holds: fn(&S) -> bool,
    ) -> Self {
        Self {
            property_id,
            property_kind: crate::ir::PropertyKind::Invariant,
            expr,
            holds,
        }
    }
}

pub trait ModelSpec {
    type State: StateSpec;
    type Action: ActionSpec;

    fn model_id() -> &'static str;
    fn init_states() -> Vec<Self::State>;
    fn step(state: &Self::State, action: &Self::Action) -> Vec<Self::State>;
    fn properties() -> Vec<ModelProperty<Self::State>>;
    fn transitions() -> Vec<TransitionDescriptor> {
        Vec::new()
    }

    fn observe(state: &Self::State) -> BTreeMap<String, Value> {
        state.snapshot()
    }

    fn enabled_actions(state: &Self::State) -> Vec<Self::Action> {
        Self::Action::all()
            .into_iter()
            .filter(|action| !Self::step(state, action).is_empty())
            .collect()
    }
}

pub trait VerifiedMachine: ModelSpec {}

impl<T> VerifiedMachine for T where T: ModelSpec {}

fn primary_property<M: ModelSpec>() -> ModelProperty<M::State> {
    M::properties()
        .into_iter()
        .next()
        .expect("ModelSpec::properties must return at least one property")
}

fn find_property<M: ModelSpec>(property_id: &str) -> ModelProperty<M::State> {
    M::properties()
        .into_iter()
        .find(|property| property.property_id == property_id)
        .unwrap_or_else(|| {
            panic!(
                "unknown property `{property_id}` for model `{}`",
                M::model_id()
            )
        })
}

pub fn property_ids<M: ModelSpec>() -> Vec<&'static str> {
    M::properties()
        .into_iter()
        .map(|property| property.property_id)
        .collect()
}

pub fn state_field_descriptors<S: StateSpec>() -> Vec<StateFieldDescriptor> {
    S::state_fields()
}

pub fn action_descriptors<A: ActionSpec>() -> Vec<ActionDescriptor> {
    A::action_descriptors()
}

pub fn transition_descriptors<M: ModelSpec>() -> Vec<TransitionDescriptor> {
    M::transitions()
}

pub fn machine_transition_ir<M: ModelSpec>() -> Vec<MachineTransitionIr> {
    let descriptors = M::transitions();
    if !descriptors.is_empty() {
        return descriptors
            .into_iter()
            .map(|descriptor| MachineTransitionIr {
                action_variant: descriptor.action_variant,
                action_id: descriptor.action_id,
                guard: Some(descriptor.guard),
                effect: Some(descriptor.effect),
                reads: descriptor.reads,
                writes: descriptor.writes,
                path_tags: descriptor.path_tags.to_vec(),
                updates: descriptor
                    .updates
                    .iter()
                    .map(|update| MachineTransitionUpdateIr {
                        field: update.field,
                        expr: Some(update.expr),
                    })
                    .collect(),
            })
            .collect();
    }

    M::Action::action_descriptors()
        .into_iter()
        .map(|descriptor| MachineTransitionIr {
            action_variant: descriptor.variant,
            action_id: descriptor.action_id,
            guard: None,
            effect: None,
            reads: descriptor.reads,
            writes: descriptor.writes,
            path_tags: Vec::new(),
            updates: Vec::new(),
        })
        .collect()
}

pub fn machine_capability_report<M: VerifiedMachine>() -> MachineCapabilityReport {
    let machine_ir = lower_machine_model::<M>();
    match machine_ir {
        Ok(_) => MachineCapabilityReport {
            parse_ready: true,
            explicit_ready: true,
            ir_ready: true,
            solver_ready: true,
            coverage_ready: true,
            explain_ready: true,
            testgen_ready: true,
            machine_ir_error: None,
            reasons: Vec::new(),
        },
        Err(error) => MachineCapabilityReport {
            parse_ready: true,
            explicit_ready: true,
            ir_ready: false,
            solver_ready: false,
            coverage_ready: true,
            explain_ready: true,
            testgen_ready: true,
            machine_ir_error: Some(error.clone()),
            reasons: machine_capability_reasons::<M>(&error),
        },
    }
}

pub fn machine_transition_tags_for_action<M: ModelSpec>(action_id: &str) -> Vec<String> {
    let mut tags = BTreeSet::new();
    for transition in machine_transition_ir::<M>()
        .into_iter()
        .filter(|transition| transition.action_id == action_id)
    {
        tags.extend(decision_path_tags(
            &transition.path_tags,
            transition.action_id,
            transition.reads.iter().copied(),
            transition.writes.iter().copied(),
            transition.guard,
            transition.effect,
        ));
    }
    if tags.is_empty() {
        vec!["transition_path".to_string()]
    } else {
        tags.into_iter().collect()
    }
}

fn machine_capability_reasons<M: VerifiedMachine>(error: &str) -> Vec<String> {
    let mut reasons = Vec::new();
    if M::transitions().is_empty() {
        reasons.push("opaque_step_closure".to_string());
    }
    if error.contains("requires exactly one init state") {
        reasons.push("multiple_init_states".to_string());
    }
    if error.contains("declarative transitions") {
        reasons.push("missing_declarative_transitions".to_string());
    }
    if error.contains("unsupported machine guard expression") {
        reasons.push("unsupported_machine_guard_expr".to_string());
    }
    if error.contains("unsupported machine update expression") {
        reasons.push("unsupported_machine_update_expr".to_string());
    }
    if error.contains("not representable in the current IR subset") {
        reasons.push("unsupported_machine_property_expr".to_string());
    }
    if error.contains("unsupported rust field type") {
        reasons.push("unsupported_rust_field_type".to_string());
    }
    if error.contains("exceeds supported u8 bounds") {
        reasons.push("unsupported_field_range".to_string());
    }
    if reasons.is_empty() {
        reasons.push("machine_ir_lowering_failed".to_string());
    }
    reasons
}

#[doc(hidden)]
pub fn action_descriptor_by_variant<A: ActionSpec>(variant: &str) -> ActionDescriptor {
    A::action_descriptors()
        .into_iter()
        .find(|descriptor| descriptor.variant == variant)
        .unwrap_or_else(|| panic!("unknown action variant `{variant}`"))
}

#[macro_export]
macro_rules! valid_state {
    (
        struct $state:ident {
            $($field:ident : $field_ty:ty $( [ $($meta:tt)+ ] )? ),+ $(,)?
        }
    ) => {
        #[derive(Debug, Clone, PartialEq, Eq, Hash)]
        struct $state {
            $( $field: $field_ty, )+
        }

        impl $crate::modeling::ModelingState for $state {
            fn snapshot(&self) -> std::collections::BTreeMap<String, $crate::ir::Value> {
                std::collections::BTreeMap::from([
                    $(
                        (
                            stringify!($field).to_string(),
                            $crate::modeling::IntoModelValue::into_model_value(self.$field.clone()),
                        )
                    ),+
                ])
            }
        }

        impl $crate::modeling::StateSpec for $state {
            fn state_fields() -> Vec<$crate::modeling::StateFieldDescriptor> {
                vec![
                    $(
                        $crate::modeling::StateFieldDescriptor {
                            name: stringify!($field),
                            rust_type: stringify!($field_ty),
                            range: $crate::valid_state!(@range $($($meta)+)?),
                            variants: $crate::valid_state!(@variants [$field_ty] $( $($meta)+ )?),
                            is_set: $crate::valid_state!(@is_set $( $($meta)+ )?),
                        }
                    ),+
                ]
            }
        }
    };
    (@range range = $range:literal) => {
        Some($range)
    };
    (@range enum) => {
        None
    };
    (@range set) => {
        None
    };
    (@range) => {
        None
    };
    (@variants [$field_ty:ty] enum) => {
        Some(<$field_ty as $crate::modeling::FiniteValueSpec>::variant_labels().to_vec())
    };
    (@variants [$field_ty:ty] set) => {
        Some(<$field_ty as $crate::modeling::FiniteSetSpec>::variant_labels().to_vec())
    };
    (@variants [$field_ty:ty] range = $range:literal) => {
        None
    };
    (@variants [$field_ty:ty]) => {
        None
    };
    (@is_set set) => {
        true
    };
    (@is_set enum) => {
        false
    };
    (@is_set range = $range:literal) => {
        false
    };
    (@is_set) => {
        false
    };
}

#[macro_export]
macro_rules! valid_state_spec {
    (
        $state:ty {
            $($field:ident : $field_ty:ty $( [ $($meta:tt)+ ] )? ),+ $(,)?
        }
    ) => {
        impl $crate::modeling::ModelingState for $state {
            fn snapshot(&self) -> std::collections::BTreeMap<String, $crate::ir::Value> {
                std::collections::BTreeMap::from([
                    $(
                        (
                            stringify!($field).to_string(),
                            $crate::modeling::IntoModelValue::into_model_value(self.$field.clone()),
                        )
                    ),+
                ])
            }
        }

        impl $crate::modeling::StateSpec for $state {
            fn state_fields() -> Vec<$crate::modeling::StateFieldDescriptor> {
                vec![
                    $(
                        $crate::modeling::StateFieldDescriptor {
                            name: stringify!($field),
                            rust_type: stringify!($field_ty),
                            range: $crate::valid_state!(@range $($($meta)+)?),
                            variants: $crate::valid_state!(@variants [$field_ty] $( $($meta)+ )?),
                            is_set: $crate::valid_state!(@is_set $( $($meta)+ )?),
                        }
                    ),+
                ]
            }
        }
    };
}

#[macro_export]
macro_rules! valid_actions {
    (
        enum $action:ident {
            $(
                $variant:ident => $action_id:literal
                $( [ reads = [$($read:literal),* $(,)?], writes = [$($write:literal),* $(,)?] ] )?
            ),+ $(,)?
        }
    ) => {
        #[derive(Debug, Clone, PartialEq, Eq, Hash)]
        enum $action {
            $( $variant, )+
        }

        impl $crate::modeling::Finite for $action {
            fn all() -> Vec<Self> {
                vec![$(Self::$variant),+]
            }
        }

        impl $crate::modeling::ModelingAction for $action {
            fn action_id(&self) -> String {
                match self {
                    $( Self::$variant => $action_id.to_string(), )+
                }
            }
        }

        impl $crate::modeling::ActionSpec for $action {
            fn action_descriptors() -> Vec<$crate::modeling::ActionDescriptor> {
                vec![
                    $(
                        $crate::modeling::ActionDescriptor {
                            variant: stringify!($variant),
                            action_id: $action_id,
                            reads: $crate::valid_actions!(@reads $($($read),*)?),
                            writes: $crate::valid_actions!(@writes $($($write),*)?),
                        }
                    ),+
                ]
            }
        }
    };
    (@reads $($read:literal),*) => {
        &[$($read),*]
    };
    (@writes $($write:literal),*) => {
        &[$($write),*]
    };
}

#[macro_export]
macro_rules! valid_action_spec {
    (
        $action:ty {
            $(
                $variant:ident => $action_id:literal
                $( [ reads = [$($read:literal),* $(,)?], writes = [$($write:literal),* $(,)?] ] )?
            ),+ $(,)?
        }
    ) => {
        impl $crate::modeling::Finite for $action {
            fn all() -> Vec<Self> {
                vec![$(<$action>::$variant),+]
            }
        }

        impl $crate::modeling::ModelingAction for $action {
            fn action_id(&self) -> String {
                match self {
                    $( <$action>::$variant => $action_id.to_string(), )+
                }
            }
        }

        impl $crate::modeling::ActionSpec for $action {
            fn action_descriptors() -> Vec<$crate::modeling::ActionDescriptor> {
                vec![
                    $(
                        $crate::modeling::ActionDescriptor {
                            variant: stringify!($variant),
                            action_id: $action_id,
                            reads: $crate::valid_actions!(@reads $($($read),*)?),
                            writes: $crate::valid_actions!(@writes $($($write),*)?),
                        }
                    ),+
                ]
            }
        }
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! valid_model {
    (
        model $model:ident<$state_ty:ty, $action_ty:ty>;
        init [$($init_state:expr),* $(,)?];
        transitions {
            $(
                on $group_action:ident {
                    $(
                        $( [ tags = [$($path_tag:literal),* $(,)?] ] )?
                        when |$guard_state:ident| $guard_expr:expr
                        => [$state_ctor:ident { $($field:ident : $update_expr:expr),* $(,)? }];
                    )+
                }
            )+
        }
        properties {
            $(invariant $property:ident |$holds_state:ident| $holds_expr:expr;)+
        }
    ) => {
        $crate::valid_model! {
            model $model<$state_ty, $action_ty>;
            init [$($init_state),*];
            transitions {
                $(
                    $(
                        transition $group_action $( [ tags = [$($path_tag),*] ] )? when |$guard_state| $guard_expr => [$state_ctor { $($field: $update_expr),* }];
                    )+
                )+
            }
            properties {
                $(invariant $property |$holds_state| $holds_expr;)+
            }
        }
    };
    (
        model $model:ident<$state_ty:ty, $action_ty:ty>;
        init [$($init_state:expr),* $(,)?];
        transitions {
            $(
                on $group_action:ident {
                    $(
                        $( [ tags = [$($path_tag:literal),* $(,)?] ] )?
                        when |$guard_state:ident| $guard_expr:expr
                        => [$($next_state:expr),* $(,)?];
                    )+
                }
            )+
        }
        properties {
            $(invariant $property:ident |$holds_state:ident| $holds_expr:expr;)+
        }
    ) => {
        $crate::valid_model! {
            model $model<$state_ty, $action_ty>;
            init [$($init_state),*];
            transitions {
                $(
                    $(
                        transition $group_action $( [ tags = [$($path_tag),*] ] )? when |$guard_state| $guard_expr => [$($next_state),*];
                    )+
                )+
            }
            properties {
                $(invariant $property |$holds_state| $holds_expr;)+
            }
        }
    };
    (
        model $model:ident<$state_ty:ty, $action_ty:ty>;
        init [$($init_state:expr),* $(,)?];
        transitions {
            $(transition $transition_action:ident $( [ tags = [$($path_tag:literal),* $(,)?] ] )? when |$guard_state:ident| $guard_expr:expr => [$state_ctor:ident { $($field:ident : $update_expr:expr),* $(,)? }];)+
        }
        properties {
            $(invariant $property:ident |$holds_state:ident| $holds_expr:expr;)+
        }
    ) => {
        struct $model;

        impl $crate::modeling::ModelSpec for $model {
            type State = $state_ty;
            type Action = $action_ty;

            fn model_id() -> &'static str {
                stringify!($model)
            }

            fn init_states() -> Vec<Self::State> {
                vec![$($init_state),*]
            }

            fn step(state: &Self::State, action: &Self::Action) -> Vec<Self::State> {
                let mut next_states = Vec::new();
                $(
                    if matches!(action, <$action_ty>::$transition_action) {
                        let $guard_state = state;
                        if $guard_expr {
                            next_states.push($state_ctor { $($field: $update_expr),* });
                        }
                    }
                )+
                next_states
            }

            fn properties() -> Vec<$crate::modeling::ModelProperty<Self::State>> {
                vec![
                    $(
                        $crate::modeling::ModelProperty::invariant_expr(
                            stringify!($property),
                            Some(stringify!($holds_expr)),
                            |$holds_state: &Self::State| $holds_expr,
                        )
                    ),+
                ]
            }

            fn transitions() -> Vec<$crate::modeling::TransitionDescriptor> {
                vec![
                    $(
                        {
                            let descriptor = $crate::modeling::action_descriptor_by_variant::<$action_ty>(
                                stringify!($transition_action)
                            );
                            $crate::modeling::TransitionDescriptor {
                                action_variant: descriptor.variant,
                                action_id: descriptor.action_id,
                                guard: stringify!($guard_expr),
                                effect: stringify!($state_ctor { $($field: $update_expr),* }),
                                reads: descriptor.reads,
                                writes: descriptor.writes,
                                path_tags: $crate::valid_model!(@path_tags $($($path_tag),*)?),
                                updates: &[
                                    $(
                                        $crate::modeling::TransitionUpdateDescriptor {
                                            field: stringify!($field),
                                            expr: stringify!($update_expr),
                                        }
                                    ),*
                                ],
                            }
                        }
                    ),+
                ]
            }
        }
    };
    (
        model $model:ident<$state_ty:ty, $action_ty:ty>;
        init [$($init_state:expr),* $(,)?];
        transitions {
            $(transition $transition_action:ident $( [ tags = [$($path_tag:literal),* $(,)?] ] )? when |$guard_state:ident| $guard_expr:expr => [$($next_state:expr),* $(,)?];)+
        }
        properties {
            $(invariant $property:ident |$holds_state:ident| $holds_expr:expr;)+
        }
    ) => {
        struct $model;

        impl $crate::modeling::ModelSpec for $model {
            type State = $state_ty;
            type Action = $action_ty;

            fn model_id() -> &'static str {
                stringify!($model)
            }

            fn init_states() -> Vec<Self::State> {
                vec![$($init_state),*]
            }

            fn step(state: &Self::State, action: &Self::Action) -> Vec<Self::State> {
                let mut next_states = Vec::new();
                $(
                    if matches!(action, <$action_ty>::$transition_action) {
                        let $guard_state = state;
                        if $guard_expr {
                            next_states.extend(vec![$($next_state),*]);
                        }
                    }
                )+
                next_states
            }

            fn properties() -> Vec<$crate::modeling::ModelProperty<Self::State>> {
                vec![
                    $(
                        $crate::modeling::ModelProperty::invariant_expr(
                            stringify!($property),
                            Some(stringify!($holds_expr)),
                            |$holds_state: &Self::State| $holds_expr,
                        )
                    ),+
                ]
            }

            fn transitions() -> Vec<$crate::modeling::TransitionDescriptor> {
                vec![
                    $(
                        {
                            let descriptor = $crate::modeling::action_descriptor_by_variant::<$action_ty>(
                                stringify!($transition_action)
                            );
                            $crate::modeling::TransitionDescriptor {
                                action_variant: descriptor.variant,
                                action_id: descriptor.action_id,
                                guard: stringify!($guard_expr),
                                effect: stringify!([$($next_state),*]),
                                reads: descriptor.reads,
                                writes: descriptor.writes,
                                path_tags: $crate::valid_model!(@path_tags $($($path_tag),*)?),
                                updates: &[],
                            }
                        }
                    ),+
                ]
            }
        }
    };
    (@path_tags $($path_tag:literal),*) => {
        &[$($path_tag),*]
    };
    (@path_tags) => {
        &[]
    };
    (
        model $model:ident<$state_ty:ty, $action_ty:ty>;
        init [$($init_state:expr),* $(,)?];
        step |$state:ident, $action:ident| $step_body:block
        properties {
            $(invariant $property:ident |$holds_state:ident| $holds_expr:expr;)+
        }
    ) => {
        struct $model;

        impl $crate::modeling::ModelSpec for $model {
            type State = $state_ty;
            type Action = $action_ty;

            fn model_id() -> &'static str {
                stringify!($model)
            }

            fn init_states() -> Vec<Self::State> {
                vec![$($init_state),*]
            }

            fn step($state: &Self::State, $action: &Self::Action) -> Vec<Self::State> $step_body

            fn properties() -> Vec<$crate::modeling::ModelProperty<Self::State>> {
                vec![
                    $(
                        $crate::modeling::ModelProperty::invariant_expr(
                            stringify!($property),
                            Some(stringify!($holds_expr)),
                            |$holds_state: &Self::State| $holds_expr,
                        )
                    ),+
                ]
            }
        }
    };
    (
        model $model:ident<$state_ty:ty, $action_ty:ty>;
        property $property:ident;
        init [$($init_state:expr),* $(,)?];
        step |$state:ident, $action:ident| $step_body:block
        invariant |$holds_state:ident| $holds_expr:expr;
    ) => {
        $crate::valid_model! {
            model $model<$state_ty, $action_ty>;
            init [$($init_state),*];
            step |$state, $action| $step_body
            properties {
                invariant $property |$holds_state| $holds_expr;
            }
        }
    };
    (
        model $model:ident;
        $($rest:tt)*
    ) => {
        compile_error!("valid_model! requires explicit state/action types. Use `model Name<State, Action>;`.");
    };
    ($($rest:tt)*) => {
        compile_error!("invalid valid_model! syntax. Expected `model Name<State, Action>; init [...]; ...`.");
    };
}

#[derive(Debug, Clone)]
struct ModelingNode<S, A> {
    state: S,
    parent: Option<usize>,
    via_action: Option<A>,
    depth: u32,
}

#[derive(Debug, Clone)]
struct ModelingEdge<S, A> {
    from_index: usize,
    to_index: usize,
    action: A,
    state_before: S,
    state_after: S,
}

pub fn check_machine<M: VerifiedMachine>() -> ModelingCheckResult<M::State, M::Action> {
    let property = primary_property::<M>();
    check_machine_property::<M>(property.property_id)
}

pub fn check_machine_property<M: VerifiedMachine>(
    property_id: &str,
) -> ModelingCheckResult<M::State, M::Action> {
    let property = find_property::<M>(property_id);
    let exploration = explore_machine::<M>(property.holds);
    if let Some(failure_index) = exploration.failure_index {
        return ModelingCheckResult {
            model_id: M::model_id(),
            property_id: property.property_id,
            status: ModelingRunStatus::Fail,
            explored_states: exploration.visited_states,
            explored_transitions: exploration.explored_transitions,
            trace: build_trace::<M>(&exploration.nodes, failure_index),
        };
    }
    ModelingCheckResult {
        model_id: M::model_id(),
        property_id: property.property_id,
        status: ModelingRunStatus::Pass,
        explored_states: exploration.visited_states,
        explored_transitions: exploration.explored_transitions,
        trace: Vec::new(),
    }
}

pub fn collect_machine_coverage<M: VerifiedMachine>() -> CoverageReport {
    let exploration = explore_machine::<M>(primary_property::<M>().holds);
    let total_actions = M::Action::all()
        .into_iter()
        .map(|action| action.action_id())
        .collect::<BTreeSet<_>>();
    let mut covered_actions = BTreeSet::new();
    let mut action_execution_counts = BTreeMap::new();
    let mut guard_true_actions = BTreeSet::new();
    let mut guard_false_actions = BTreeSet::new();
    let mut guard_true_counts = BTreeMap::new();
    let mut guard_false_counts = BTreeMap::new();
    let mut path_tag_counts = BTreeMap::new();
    let mut depth_histogram = BTreeMap::new();
    let mut repeated_state_count = 0usize;
    let transition_ir = machine_transition_ir::<M>();

    for node in &exploration.nodes {
        *depth_histogram.entry(node.depth).or_insert(0) += 1;
        for action in M::Action::all() {
            let next_states = M::step(&node.state, &action);
            if next_states.is_empty() {
                guard_false_actions.insert(action.action_id());
                *guard_false_counts.entry(action.action_id()).or_insert(0) += 1;
            } else {
                guard_true_actions.insert(action.action_id());
                *guard_true_counts.entry(action.action_id()).or_insert(0) += 1;
            }
        }
    }

    for edge in &exploration.edges {
        let action_id = edge.action.action_id();
        covered_actions.insert(action_id.clone());
        *action_execution_counts.entry(action_id).or_insert(0) += 1;
        if let Some(transition) = transition_ir
            .iter()
            .find(|transition| transition.action_id == edge.action.action_id())
        {
            for tag in decision_path_tags(
                &transition.path_tags,
                transition.action_id,
                transition.reads.iter().copied(),
                transition.writes.iter().copied(),
                transition.guard,
                transition.effect,
            ) {
                *path_tag_counts.entry(tag).or_insert(0) += 1;
            }
        }
        if edge.to_index <= edge.from_index {
            repeated_state_count += 1;
        }
    }

    let transition_coverage_percent = if total_actions.is_empty() {
        100
    } else {
        ((covered_actions.len() * 100) / total_actions.len()) as u32
    };
    let fully_covered_guards = total_actions
        .iter()
        .filter(|action_id| {
            guard_true_actions.contains(*action_id) && guard_false_actions.contains(*action_id)
        })
        .count();
    let guard_full_coverage_percent = if total_actions.is_empty() {
        100
    } else {
        ((fully_covered_guards * 100) / total_actions.len()) as u32
    };
    let uncovered_guards = total_actions
        .iter()
        .filter_map(|action_id| {
            if guard_true_actions.contains(action_id) && guard_false_actions.contains(action_id) {
                None
            } else if guard_true_actions.contains(action_id) {
                Some(format!("{action_id}:false"))
            } else if guard_false_actions.contains(action_id) {
                Some(format!("{action_id}:true"))
            } else {
                Some(format!("{action_id}:true,false"))
            }
        })
        .collect::<Vec<_>>();

    CoverageReport {
        schema_version: "1.0.0".to_string(),
        model_id: M::model_id().to_string(),
        transition_coverage_percent,
        guard_full_coverage_percent,
        covered_actions,
        total_actions,
        action_execution_counts,
        visited_state_count: exploration.nodes.len(),
        repeated_state_count,
        max_depth_observed: exploration
            .nodes
            .iter()
            .map(|node| node.depth)
            .max()
            .unwrap_or(0),
        guard_true_actions,
        guard_false_actions,
        guard_true_counts,
        guard_false_counts,
        uncovered_guards,
        path_tag_counts,
        depth_histogram,
        step_count: exploration.edges.len(),
    }
}

pub fn explain_machine<M: VerifiedMachine>(request_id: &str) -> Result<ExplainResponse, String> {
    let outcome = check_machine_outcome::<M>(request_id);
    let CheckOutcome::Completed(result) = outcome else {
        return Err("modeling adapter returned an error outcome".to_string());
    };
    let trace = result
        .trace
        .ok_or_else(|| "no evidence trace available for explain".to_string())?;
    let failure_step = trace
        .steps
        .last()
        .ok_or_else(|| "empty trace cannot be explained".to_string())?;
    let involved_fields = failure_step
        .state_before
        .iter()
        .filter_map(|(field, before)| {
            let after = failure_step.state_after.get(field)?;
            if before != after {
                Some(field.clone())
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    let coverage = collect_machine_coverage::<M>();
    let action_id = failure_step
        .action_id
        .clone()
        .unwrap_or_else(|| "INITIAL".to_string());
    let transition = machine_transition_ir::<M>()
        .into_iter()
        .find(|transition| transition.action_id == action_id);
    let mut action_reads = Vec::new();
    let mut action_writes = Vec::new();
    let mut action_path_tags = Vec::new();
    let mut write_overlap_fields = Vec::new();
    let mut candidate_causes = Vec::new();
    if involved_fields.is_empty() {
        candidate_causes.push(ExplainCandidateCause {
            kind: "terminal_violation".to_string(),
            message: format!(
                "property {} failed without a visible field diff at the terminal state",
                trace.property_id
            ),
        });
    } else {
        candidate_causes.extend(involved_fields.iter().map(|field| ExplainCandidateCause {
            kind: "field_flip".to_string(),
            message: format!("{field} changed at step {}", failure_step.index),
        }));
    }
    let execution_count = coverage
        .action_execution_counts
        .get(&action_id)
        .copied()
        .unwrap_or(0);
    if execution_count <= 1 {
        candidate_causes.push(ExplainCandidateCause {
            kind: "rare_action_path".to_string(),
            message: format!(
                "action {action_id} was executed only {} time across the reachable state space",
                execution_count
            ),
        });
    }
    if let Some(uncovered) = coverage
        .uncovered_guards
        .iter()
        .find(|entry| entry.starts_with(&format!("{action_id}:")))
    {
        candidate_causes.push(ExplainCandidateCause {
            kind: "guard_polarity_gap".to_string(),
            message: format!("guard coverage for action {action_id} is incomplete: {uncovered}"),
        });
    }
    if let Some(transition) = transition {
        action_reads = transition
            .reads
            .iter()
            .map(|item| item.to_string())
            .collect();
        action_writes = transition
            .writes
            .iter()
            .map(|item| item.to_string())
            .collect();
        write_overlap_fields = involved_fields
            .iter()
            .filter(|field| transition.writes.contains(&field.as_str()))
            .cloned()
            .collect();
        let path_tags = decision_path_tags(
            &transition.path_tags,
            transition.action_id,
            transition.reads.iter().copied(),
            transition.writes.iter().copied(),
            transition.guard,
            transition.effect,
        );
        action_path_tags = path_tags.clone();
        if !path_tags.is_empty() {
            candidate_causes.push(ExplainCandidateCause {
                kind: "decision_path_tags".to_string(),
                message: format!(
                    "action {action_id} participates in path tags [{}]",
                    path_tags.join(", ")
                ),
            });
        }
    }
    let mut repair_hints = vec![
        "review the action semantics that lead into the violating state".to_string(),
        format!("verify invariant {} is intended", trace.property_id),
    ];
    if !write_overlap_fields.is_empty() {
        repair_hints.push(format!(
            "narrow or guard writes [{}] in the failing action",
            write_overlap_fields.join(", ")
        ));
    }
    if !involved_fields.is_empty() {
        repair_hints.push(format!(
            "focus on fields [{}] when reviewing the failing transition",
            involved_fields.join(", ")
        ));
    }
    let next_steps = vec![
        "inspect the model graph to review guard and update structure".to_string(),
        "generate a regression test from the failing path".to_string(),
        "review readiness output if solver lowering is expected".to_string(),
    ];
    let confidence = (0.45_f32
        + if !involved_fields.is_empty() {
            0.2_f32
        } else {
            0.0_f32
        }
        + if execution_count <= 1 {
            0.15_f32
        } else {
            0.0_f32
        }
        + if coverage
            .uncovered_guards
            .iter()
            .any(|entry| entry.starts_with(&format!("{action_id}:")))
        {
            0.1_f32
        } else {
            0.0_f32
        })
    .min(0.95_f32);

    Ok(ExplainResponse {
        schema_version: "1.0.0".to_string(),
        request_id: request_id.to_string(),
        status: "ok".to_string(),
        evidence_id: trace.evidence_id,
        property_id: trace.property_id,
        failure_step_index: failure_step.index,
        failing_action_id: Some(action_id.clone()),
        failing_action_reads: action_reads,
        failing_action_writes: action_writes,
        failing_action_path_tags: action_path_tags,
        write_overlap_fields,
        involved_fields,
        candidate_causes,
        repair_hints,
        next_steps,
        confidence,
        best_practices: vec![
            "keep actions small so violating transitions stay explainable".to_string(),
            "cover both enabled and disabled outcomes of critical actions".to_string(),
        ],
    })
}

pub fn build_machine_test_vectors<M: VerifiedMachine>() -> Vec<TestVector> {
    let property = primary_property::<M>();
    build_machine_test_vectors_for_property::<M>(property.property_id)
}

pub fn build_machine_test_vectors_for_property<M: VerifiedMachine>(
    property_id: &str,
) -> Vec<TestVector> {
    let property = find_property::<M>(property_id);
    let exploration = explore_machine::<M>(property.holds);
    if let Some(failure_index) = exploration.failure_index {
        let trace = build_evidence_trace::<M>(
            "req-modeling",
            &modeling_result_from_failure::<M>(&exploration, failure_index, property.property_id),
        );
        return build_counterexample_vector(&trace)
            .map(|vector| vec![vector])
            .unwrap_or_default();
    }

    let mut seen_sequences = BTreeSet::new();
    let mut vectors = Vec::new();
    for edge in &exploration.edges {
        let first_sequence = vec![edge.action.action_id()];
        if seen_sequences.insert(first_sequence.clone()) {
            vectors.push(TestVector {
                schema_version: "1.0.0".to_string(),
                vector_id: format!(
                    "vec-{}",
                    stable_hash_hex(&(M::model_id().to_string() + &first_sequence.join(",")))
                        .replace("sha256:", "")
                ),
                source_kind: "witness".to_string(),
                strictness: "heuristic".to_string(),
                derivation: "transition_search".to_string(),
                evidence_id: None,
                strategy: "witness".to_string(),
                generator_version: env!("CARGO_PKG_VERSION").to_string(),
                seed: None,
                actions: vec![VectorActionStep {
                    index: 0,
                    action_id: edge.action.action_id(),
                    action_label: edge.action.action_label(),
                }],
                initial_state: Some(edge.state_before.snapshot()),
                expected_states: vec![format!("{:?}", edge.state_after.snapshot())],
                property_id: property.property_id.to_string(),
                minimized: false,
                focus_action_id: Some(edge.action.action_id()),
                focus_field: None,
                expected_guard_enabled: Some(true),
                notes: machine_transition_tags_for_action::<M>(&edge.action.action_id())
                    .into_iter()
                    .map(|tag| format!("path_tag:{tag}"))
                    .collect(),
                replay_target: None,
            });
        }
    }
    vectors
}

pub fn build_machine_test_vectors_for_strategy<M: VerifiedMachine>(
    property_id: Option<&str>,
    strategy: &str,
) -> Vec<TestVector> {
    let property_id = property_id.unwrap_or_else(|| primary_property::<M>().property_id);
    match strategy {
        "counterexample" => build_machine_test_vectors_for_property::<M>(property_id)
            .into_iter()
            .filter(|vector| vector.strategy == "counterexample")
            .collect(),
        "transition" | "witness" => build_transition_witness_vectors::<M>(property_id),
        "path" => build_path_tag_vectors::<M>(property_id),
        "guard" => build_guard_coverage_vectors::<M>(property_id),
        "boundary" => build_boundary_focus_vectors::<M>(property_id),
        "random" => build_randomized_vectors::<M>(property_id, 5),
        _ => build_machine_test_vectors_for_property::<M>(property_id),
    }
}

fn build_transition_witness_vectors<M: VerifiedMachine>(property_id: &str) -> Vec<TestVector> {
    build_machine_test_vectors_for_property::<M>(property_id)
        .into_iter()
        .filter(|vector| vector.source_kind == "witness")
        .collect()
}

fn build_path_tag_vectors<M: VerifiedMachine>(property_id: &str) -> Vec<TestVector> {
    let property = find_property::<M>(property_id);
    let exploration = explore_machine::<M>(property.holds);
    let mut vectors = Vec::new();
    let mut seen = BTreeSet::new();

    for descriptor in machine_transition_ir::<M>() {
        let tags = machine_transition_tags_for_action::<M>(descriptor.action_id);
        let Some(edge) = exploration
            .edges
            .iter()
            .find(|edge| edge.action.action_id() == descriptor.action_id)
        else {
            continue;
        };
        for tag in tags {
            if let Some(vector) = build_machine_vector_for_node::<M>(
                &exploration.nodes,
                edge.to_index,
                property.property_id,
                "path",
                "path",
                Some(descriptor.action_id.to_string()),
                None,
                Some(true),
                vec![format!("path_tag:{tag}")],
            ) {
                let signature = (
                    tag,
                    vector.focus_action_id.clone(),
                    vector
                        .actions
                        .iter()
                        .map(|s| s.action_id.clone())
                        .collect::<Vec<_>>(),
                );
                if seen.insert(signature) {
                    vectors.push(vector);
                }
            }
        }
    }

    vectors
}

fn build_guard_coverage_vectors<M: VerifiedMachine>(property_id: &str) -> Vec<TestVector> {
    let property = find_property::<M>(property_id);
    let exploration = explore_machine::<M>(property.holds);
    let transition_ir = machine_transition_ir::<M>();
    if transition_ir.is_empty()
        || transition_ir
            .iter()
            .all(|transition| transition.guard.is_none())
    {
        return build_transition_witness_vectors::<M>(property_id);
    }

    let actions = M::Action::all()
        .into_iter()
        .map(|action| (action.action_id(), action))
        .collect::<BTreeMap<_, _>>();
    let mut vectors = Vec::new();
    let mut seen = BTreeSet::new();

    for descriptor in transition_ir {
        if let Some(edge) = exploration
            .edges
            .iter()
            .find(|edge| edge.action.action_id() == descriptor.action_id)
        {
            if let Some(vector) = build_machine_vector_for_node::<M>(
                &exploration.nodes,
                edge.to_index,
                property.property_id,
                "guard",
                "guard",
                Some(descriptor.action_id.to_string()),
                None,
                Some(true),
                {
                    let mut notes = vec![format!(
                        "guard_true: {}",
                        descriptor.guard.unwrap_or("unknown")
                    )];
                    notes.extend(
                        machine_transition_tags_for_action::<M>(descriptor.action_id)
                            .into_iter()
                            .map(|tag| format!("path_tag:{tag}")),
                    );
                    notes
                },
            ) {
                let signature = (
                    vector.focus_action_id.clone(),
                    vector.expected_guard_enabled,
                    vector
                        .actions
                        .iter()
                        .map(|s| s.action_id.clone())
                        .collect::<Vec<_>>(),
                );
                if seen.insert(signature) {
                    vectors.push(vector);
                }
            }
        }

        let Some(action) = actions.get(descriptor.action_id) else {
            continue;
        };
        if let Some((node_index, _)) = exploration
            .nodes
            .iter()
            .enumerate()
            .find(|(_, node)| M::step(&node.state, action).is_empty())
        {
            if let Some(vector) = build_machine_vector_for_node::<M>(
                &exploration.nodes,
                node_index,
                property.property_id,
                "guard",
                "guard",
                Some(descriptor.action_id.to_string()),
                None,
                Some(false),
                {
                    let mut notes = vec![format!(
                        "guard_false: {}",
                        descriptor.guard.unwrap_or("unknown")
                    )];
                    notes.extend(
                        machine_transition_tags_for_action::<M>(descriptor.action_id)
                            .into_iter()
                            .map(|tag| format!("path_tag:{tag}")),
                    );
                    notes
                },
            ) {
                let signature = (
                    vector.focus_action_id.clone(),
                    vector.expected_guard_enabled,
                    vector
                        .actions
                        .iter()
                        .map(|s| s.action_id.clone())
                        .collect::<Vec<_>>(),
                );
                if seen.insert(signature) {
                    vectors.push(vector);
                }
            }
        }
    }

    vectors
}

fn build_boundary_focus_vectors<M: VerifiedMachine>(property_id: &str) -> Vec<TestVector> {
    let property = find_property::<M>(property_id);
    let exploration = explore_machine::<M>(property.holds);
    let fields = M::State::state_fields();
    let mut vectors = Vec::new();
    let mut seen = BTreeSet::new();

    for field in fields {
        let Some((min, max)) = parse_inclusive_range(field.range) else {
            continue;
        };
        for target in [min, max] {
            if let Some((node_index, _)) = exploration.nodes.iter().enumerate().find(|(_, node)| {
                matches!(
                    node.state.snapshot().get(field.name),
                    Some(Value::UInt(value)) if *value == target
                )
            }) {
                if let Some(vector) = build_machine_vector_for_node::<M>(
                    &exploration.nodes,
                    node_index,
                    property.property_id,
                    "boundary",
                    "boundary",
                    None,
                    Some(field.name.to_string()),
                    None,
                    vec![format!("boundary_target:{target}")],
                ) {
                    let signature = (
                        vector.focus_field.clone(),
                        vector.notes.clone(),
                        vector
                            .actions
                            .iter()
                            .map(|s| s.action_id.clone())
                            .collect::<Vec<_>>(),
                    );
                    if seen.insert(signature) {
                        vectors.push(vector);
                    }
                }
            }
        }
    }

    vectors
}

fn build_randomized_vectors<M: VerifiedMachine>(
    property_id: &str,
    limit: usize,
) -> Vec<TestVector> {
    let property = find_property::<M>(property_id);
    let exploration = explore_machine::<M>(property.holds);
    let mut candidates = exploration
        .nodes
        .iter()
        .enumerate()
        .filter(|(index, _)| *index > 0)
        .map(|(index, _)| {
            let steps = build_trace::<M>(&exploration.nodes, index)
                .into_iter()
                .map(|step| step.action.action_id())
                .collect::<Vec<_>>();
            let key = stable_hash_hex(&(M::model_id().to_string() + &steps.join(",")));
            (key, index)
        })
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| left.0.cmp(&right.0));

    let mut vectors = Vec::new();
    let seed = stable_hash_hex(M::model_id());
    for (_, index) in candidates.into_iter().take(limit) {
        if let Some(mut vector) = build_machine_vector_for_node::<M>(
            &exploration.nodes,
            index,
            property.property_id,
            "random",
            "random",
            None,
            None,
            None,
            vec!["deterministic_randomized_sample".to_string()],
        ) {
            vector.seed = Some(seed.bytes().fold(0u64, |acc, byte| {
                acc.wrapping_mul(131).wrapping_add(byte as u64)
            }));
            vectors.push(vector);
        }
    }
    vectors
}

fn build_machine_vector_for_node<M: VerifiedMachine>(
    nodes: &[ModelingNode<M::State, M::Action>],
    end_index: usize,
    property_id: &str,
    source_kind: &str,
    strategy: &str,
    focus_action_id: Option<String>,
    focus_field: Option<String>,
    expected_guard_enabled: Option<bool>,
    notes: Vec<String>,
) -> Option<TestVector> {
    let trace = build_trace::<M>(nodes, end_index);
    let actions = trace
        .iter()
        .enumerate()
        .map(|(index, step)| VectorActionStep {
            index,
            action_id: step.action.action_id(),
            action_label: step.action.action_label(),
        })
        .collect::<Vec<_>>();
    let expected_states = if trace.is_empty() {
        vec![format!("{:?}", nodes.get(end_index)?.state.snapshot())]
    } else {
        trace
            .iter()
            .map(|step| format!("{:?}", step.state_after.snapshot()))
            .collect::<Vec<_>>()
    };
    let signature = actions
        .iter()
        .map(|step| step.action_id.clone())
        .collect::<Vec<_>>()
        .join(",");
    Some(TestVector {
        schema_version: "1.0.0".to_string(),
        vector_id: format!(
            "vec-{}",
            stable_hash_hex(
                &(M::model_id().to_string()
                    + property_id
                    + source_kind
                    + strategy
                    + &signature
                    + &format!(
                        "{focus_action_id:?}{focus_field:?}{expected_guard_enabled:?}{notes:?}"
                    ))
            )
            .replace("sha256:", "")
        ),
        source_kind: source_kind.to_string(),
        strictness: match strategy {
            "guard" | "boundary" | "path" | "random" | "transition" | "witness" => {
                "heuristic".to_string()
            }
            _ => "heuristic".to_string(),
        },
        derivation: match strategy {
            "guard" => "guard_search".to_string(),
            "boundary" => "boundary_search".to_string(),
            "path" => "path_tag_search".to_string(),
            "random" => "deterministic_random_search".to_string(),
            "transition" | "witness" => "transition_search".to_string(),
            _ => "model_exploration".to_string(),
        },
        evidence_id: None,
        strategy: strategy.to_string(),
        generator_version: env!("CARGO_PKG_VERSION").to_string(),
        seed: None,
        actions,
        initial_state: Some(nodes.first()?.state.snapshot()),
        expected_states,
        property_id: property_id.to_string(),
        minimized: false,
        focus_action_id,
        focus_field,
        expected_guard_enabled,
        notes,
        replay_target: None,
    })
}

fn parse_inclusive_range(range: Option<&'static str>) -> Option<(u64, u64)> {
    let range = range?;
    let (min, max) = range.split_once("..=")?;
    let min = min.parse::<u64>().ok()?;
    let max = max.parse::<u64>().ok()?;
    Some((min, max))
}

pub fn lower_machine_model<M: VerifiedMachine>() -> Result<ModelIr, String> {
    let init_states = M::init_states();
    if init_states.len() != 1 {
        return Err("machine IR lowering currently requires exactly one init state".to_string());
    }
    let init_state = init_states
        .into_iter()
        .next()
        .ok_or_else(|| "machine must define at least one init state".to_string())?;
    let snapshot = init_state.snapshot();

    let state_fields = M::State::state_fields()
        .into_iter()
        .map(|field| {
            let ty = lower_machine_field_type(&field)?;
            Ok(StateField {
                id: field.name.to_string(),
                name: field.name.to_string(),
                ty,
                span: SourceSpan { line: 1, column: 1 },
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let enum_literals = build_machine_enum_literal_map::<M>();

    let init = state_fields
        .iter()
        .map(|field| {
            let value = snapshot
                .get(&field.name)
                .cloned()
                .ok_or_else(|| format!("missing init value for field `{}`", field.name))?;
            Ok(InitAssignment {
                field: field.id.clone(),
                value,
                span: SourceSpan { line: 1, column: 1 },
            })
        })
        .collect::<Result<Vec<_>, String>>()?;

    let transitions = M::transitions();
    if transitions.is_empty() {
        return Err(
            "machine IR lowering currently requires declarative transitions { ... }".to_string(),
        );
    }

    let actions = transitions
        .into_iter()
        .map(|transition| {
            let guard = lower_machine_expr_with_enums(transition.guard, &enum_literals)
                .ok_or_else(|| {
                    format!(
                        "unsupported machine guard expression `{}`",
                        transition.guard
                    )
                })?;
            let updates = transition
                .updates
                .iter()
                .map(|update| {
                    let value = lower_machine_expr_with_enums(update.expr, &enum_literals)
                        .ok_or_else(|| {
                            format!("unsupported machine update expression `{}`", update.expr)
                        })?;
                    Ok(UpdateIr {
                        field: update.field.to_string(),
                        value,
                    })
                })
                .collect::<Result<Vec<_>, String>>()?;
            Ok(ActionIr {
                action_id: transition.action_id.to_string(),
                label: transition.action_id.to_string(),
                reads: transition
                    .reads
                    .iter()
                    .map(|item| item.to_string())
                    .collect(),
                writes: transition
                    .writes
                    .iter()
                    .map(|item| item.to_string())
                    .collect(),
                path_tags: decision_path_tags(
                    transition.path_tags,
                    transition.action_id,
                    transition.reads.iter().copied(),
                    transition.writes.iter().copied(),
                    Some(transition.guard),
                    Some(transition.effect),
                ),
                guard,
                updates,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;

    let properties = M::properties()
        .into_iter()
        .map(|property| {
            let expr = property
                .expr
                .and_then(|expr| lower_machine_expr_with_enums(expr, &enum_literals))
                .ok_or_else(|| {
                    format!(
                        "machine property `{}` is not representable in the current IR subset",
                        property.property_id
                    )
                })?;
            Ok(PropertyIr {
                property_id: property.property_id.to_string(),
                kind: property.property_kind,
                expr,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;

    Ok(ModelIr {
        model_id: M::model_id().to_string(),
        state_fields,
        init,
        actions,
        properties,
    })
}

fn lower_machine_field_type(field: &StateFieldDescriptor) -> Result<FieldType, String> {
    if let Some(variants) = &field.variants {
        if variants.len() > 64 {
            return Err(format!(
                "enum-backed finite sets currently support at most 64 variants for field `{}`",
                field.name
            ));
        }
        if field.is_set {
            return Ok(FieldType::EnumSet {
                variants: variants.iter().map(|item| item.to_string()).collect(),
            });
        }
        return Ok(FieldType::Enum {
            variants: variants.iter().map(|item| item.to_string()).collect(),
        });
    }
    match field.rust_type {
        "bool" => Ok(FieldType::Bool),
        "u8" => {
            let (min, max) = parse_inclusive_range(field.range).unwrap_or((0, u8::MAX as u64));
            if max > u8::MAX as u64 {
                return Err(format!(
                    "range `{}` exceeds supported u8 bounds for field `{}`",
                    field.range.unwrap_or("0..=255"),
                    field.name
                ));
            }
            Ok(FieldType::BoundedU8 {
                min: min as u8,
                max: max as u8,
            })
        }
        "u16" => {
            let (min, max) = parse_inclusive_range(field.range).unwrap_or((0, u16::MAX as u64));
            if max > u16::MAX as u64 {
                return Err(format!(
                    "range `{}` exceeds supported u16 bounds for field `{}`",
                    field.range.unwrap_or("0..=65535"),
                    field.name
                ));
            }
            Ok(FieldType::BoundedU16 {
                min: min as u16,
                max: max as u16,
            })
        }
        "u32" => {
            let (min, max) = parse_inclusive_range(field.range).unwrap_or((0, u32::MAX as u64));
            if max > u32::MAX as u64 {
                return Err(format!(
                    "range `{}` exceeds supported u32 bounds for field `{}`",
                    field.range.unwrap_or("0..=4294967295"),
                    field.name
                ));
            }
            Ok(FieldType::BoundedU32 {
                min: min as u32,
                max: max as u32,
            })
        }
        other => Err(format!(
            "unsupported rust field type `{other}` for machine IR lowering"
        )),
    }
}

#[cfg_attr(not(test), allow(dead_code))]
fn lower_machine_expr(input: &str) -> Option<ExprIr> {
    lower_machine_expr_with_enums(input, &BTreeMap::new())
}

fn build_machine_enum_literal_map<M: VerifiedMachine>() -> BTreeMap<String, (String, u64)> {
    let mut literals = BTreeMap::new();
    for field in M::State::state_fields() {
        if let Some(variants) = field.variants {
            let enum_ty = if field.is_set {
                set_inner_rust_type(field.rust_type).unwrap_or_else(|| field.rust_type.to_string())
            } else {
                field.rust_type.to_string()
            };
            for (index, variant) in variants.iter().enumerate() {
                literals.insert(
                    format!("{}::{}", enum_ty, variant),
                    ((*variant).to_string(), index as u64),
                );
                literals
                    .entry((*variant).to_string())
                    .or_insert_with(|| ((*variant).to_string(), index as u64));
                if !field.is_set {
                    if let Some(inner_ty) = option_inner_rust_type(field.rust_type) {
                        if let Some(inner_variant) = variant
                            .strip_prefix("Some(")
                            .and_then(|value| value.strip_suffix(')'))
                        {
                            literals.insert(
                                format!("Some({inner_ty}::{inner_variant})"),
                                ((*variant).to_string(), index as u64),
                            );
                        } else if *variant == "None" {
                            literals.insert(
                                "Option::None".to_string(),
                                ((*variant).to_string(), index as u64),
                            );
                        }
                    }
                }
            }
        }
    }
    literals
}

fn set_inner_rust_type(rust_type: &str) -> Option<String> {
    let normalized = rust_type
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .collect::<String>();
    normalized
        .strip_prefix("FiniteEnumSet<")
        .and_then(|value| value.strip_suffix('>'))
        .map(str::to_string)
        .filter(|value| !value.is_empty())
}

fn option_inner_rust_type(rust_type: &str) -> Option<String> {
    let normalized = rust_type
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .collect::<String>();
    normalized
        .strip_prefix("Option<")
        .and_then(|value| value.strip_suffix('>'))
        .map(str::to_string)
        .filter(|value| !value.is_empty())
}

fn lower_machine_expr_with_enums(
    input: &str,
    enum_literals: &BTreeMap<String, (String, u64)>,
) -> Option<ExprIr> {
    let trimmed = strip_wrapping_machine_parens(input.trim());
    let normalized = trimmed.strip_prefix("state.").unwrap_or(trimmed).trim();
    if normalized == "true" {
        return Some(ExprIr::Literal(Value::Bool(true)));
    }
    if normalized == "false" {
        return Some(ExprIr::Literal(Value::Bool(false)));
    }
    if let Some((label, index)) = enum_literals.get(normalized) {
        return Some(ExprIr::Literal(Value::EnumVariant {
            label: label.clone(),
            index: *index,
        }));
    }
    if let Some([left, right]) = function_args_machine(normalized, "implies") {
        let left = lower_machine_expr_with_enums(left, enum_literals)?;
        let right = lower_machine_expr_with_enums(right, enum_literals)?;
        return Some(ExprIr::Binary {
            op: BinaryOp::Or,
            left: Box::new(ExprIr::Unary {
                op: UnaryOp::Not,
                expr: Box::new(left),
            }),
            right: Box::new(right),
        });
    }
    if let Some([left, right]) = function_args_machine(normalized, "iff") {
        let left_expr = lower_machine_expr_with_enums(left, enum_literals)?;
        let right_expr = lower_machine_expr_with_enums(right, enum_literals)?;
        let left_and_right = ExprIr::Binary {
            op: BinaryOp::And,
            left: Box::new(left_expr.clone()),
            right: Box::new(right_expr.clone()),
        };
        let neither_left_nor_right = ExprIr::Binary {
            op: BinaryOp::And,
            left: Box::new(ExprIr::Unary {
                op: UnaryOp::Not,
                expr: Box::new(left_expr),
            }),
            right: Box::new(ExprIr::Unary {
                op: UnaryOp::Not,
                expr: Box::new(right_expr),
            }),
        };
        return Some(ExprIr::Binary {
            op: BinaryOp::Or,
            left: Box::new(left_and_right),
            right: Box::new(neither_left_nor_right),
        });
    }
    if let Some([left, right]) = function_args_machine(normalized, "xor") {
        let left_expr = lower_machine_expr_with_enums(left, enum_literals)?;
        let right_expr = lower_machine_expr_with_enums(right, enum_literals)?;
        let either = ExprIr::Binary {
            op: BinaryOp::Or,
            left: Box::new(left_expr.clone()),
            right: Box::new(right_expr.clone()),
        };
        let both = ExprIr::Binary {
            op: BinaryOp::And,
            left: Box::new(left_expr),
            right: Box::new(right_expr),
        };
        return Some(ExprIr::Binary {
            op: BinaryOp::And,
            left: Box::new(either),
            right: Box::new(ExprIr::Unary {
                op: UnaryOp::Not,
                expr: Box::new(both),
            }),
        });
    }
    if let Some([set, item]) = function_args_machine(normalized, "contains") {
        return Some(ExprIr::Binary {
            op: BinaryOp::SetContains,
            left: Box::new(lower_machine_expr_with_enums(set, enum_literals)?),
            right: Box::new(lower_machine_expr_with_enums(item, enum_literals)?),
        });
    }
    if let Some([set, item]) = function_args_machine(normalized, "insert") {
        return Some(ExprIr::Binary {
            op: BinaryOp::SetInsert,
            left: Box::new(lower_machine_expr_with_enums(set, enum_literals)?),
            right: Box::new(lower_machine_expr_with_enums(item, enum_literals)?),
        });
    }
    if let Some([set, item]) = function_args_machine(normalized, "remove") {
        return Some(ExprIr::Binary {
            op: BinaryOp::SetRemove,
            left: Box::new(lower_machine_expr_with_enums(set, enum_literals)?),
            right: Box::new(lower_machine_expr_with_enums(item, enum_literals)?),
        });
    }
    if let Some([set]) = function_args_machine(normalized, "is_empty") {
        return Some(ExprIr::Unary {
            op: UnaryOp::SetIsEmpty,
            expr: Box::new(lower_machine_expr_with_enums(set, enum_literals)?),
        });
    }
    if let Ok(value) = normalized.parse::<u64>() {
        return Some(ExprIr::Literal(Value::UInt(value)));
    }
    if let Some(rest) = normalized.strip_prefix('!') {
        return Some(ExprIr::Unary {
            op: UnaryOp::Not,
            expr: Box::new(lower_machine_expr_with_enums(rest.trim(), enum_literals)?),
        });
    }
    if let Some((left, right)) = split_top_level_machine(normalized, "||") {
        return Some(ExprIr::Binary {
            op: BinaryOp::Or,
            left: Box::new(lower_machine_expr_with_enums(left.trim(), enum_literals)?),
            right: Box::new(lower_machine_expr_with_enums(right.trim(), enum_literals)?),
        });
    }
    if let Some((left, right)) = split_top_level_machine(normalized, "&&") {
        return Some(ExprIr::Binary {
            op: BinaryOp::And,
            left: Box::new(lower_machine_expr_with_enums(left.trim(), enum_literals)?),
            right: Box::new(lower_machine_expr_with_enums(right.trim(), enum_literals)?),
        });
    }
    if let Some((left, right)) = split_top_level_machine(normalized, "!=") {
        return Some(ExprIr::Binary {
            op: BinaryOp::NotEqual,
            left: Box::new(lower_machine_expr_with_enums(left.trim(), enum_literals)?),
            right: Box::new(lower_machine_expr_with_enums(right.trim(), enum_literals)?),
        });
    }
    if let Some((left, right)) = split_top_level_machine(normalized, ">=") {
        return Some(ExprIr::Binary {
            op: BinaryOp::GreaterThanOrEqual,
            left: Box::new(lower_machine_expr_with_enums(left.trim(), enum_literals)?),
            right: Box::new(lower_machine_expr_with_enums(right.trim(), enum_literals)?),
        });
    }
    if let Some((left, right)) = split_top_level_machine(normalized, "<=") {
        return Some(ExprIr::Binary {
            op: BinaryOp::LessThanOrEqual,
            left: Box::new(lower_machine_expr_with_enums(left.trim(), enum_literals)?),
            right: Box::new(lower_machine_expr_with_enums(right.trim(), enum_literals)?),
        });
    }
    if let Some((left, right)) = split_top_level_machine(normalized, ">") {
        return Some(ExprIr::Binary {
            op: BinaryOp::GreaterThan,
            left: Box::new(lower_machine_expr_with_enums(left.trim(), enum_literals)?),
            right: Box::new(lower_machine_expr_with_enums(right.trim(), enum_literals)?),
        });
    }
    if let Some((left, right)) = split_top_level_machine(normalized, "<") {
        return Some(ExprIr::Binary {
            op: BinaryOp::LessThan,
            left: Box::new(lower_machine_expr_with_enums(left.trim(), enum_literals)?),
            right: Box::new(lower_machine_expr_with_enums(right.trim(), enum_literals)?),
        });
    }
    if let Some((left, right)) = split_top_level_machine(normalized, "==") {
        return Some(ExprIr::Binary {
            op: BinaryOp::Equal,
            left: Box::new(lower_machine_expr_with_enums(left.trim(), enum_literals)?),
            right: Box::new(lower_machine_expr_with_enums(right.trim(), enum_literals)?),
        });
    }
    if let Some((left, right)) = split_top_level_machine(normalized, "-") {
        return Some(ExprIr::Binary {
            op: BinaryOp::Sub,
            left: Box::new(lower_machine_expr_with_enums(left.trim(), enum_literals)?),
            right: Box::new(lower_machine_expr_with_enums(right.trim(), enum_literals)?),
        });
    }
    if let Some((left, right)) = split_top_level_machine(normalized, "%") {
        return Some(ExprIr::Binary {
            op: BinaryOp::Mod,
            left: Box::new(lower_machine_expr_with_enums(left.trim(), enum_literals)?),
            right: Box::new(lower_machine_expr_with_enums(right.trim(), enum_literals)?),
        });
    }
    if let Some((left, right)) = split_top_level_machine(normalized, "+") {
        return Some(ExprIr::Binary {
            op: BinaryOp::Add,
            left: Box::new(lower_machine_expr_with_enums(left.trim(), enum_literals)?),
            right: Box::new(lower_machine_expr_with_enums(right.trim(), enum_literals)?),
        });
    }
    let normalized = normalized
        .split('.')
        .next_back()
        .unwrap_or(normalized)
        .trim();
    if normalized
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
    {
        return Some(ExprIr::FieldRef(normalized.to_string()));
    }
    None
}

fn function_args_machine<'a, const N: usize>(input: &'a str, name: &str) -> Option<[&'a str; N]> {
    let call = input
        .strip_prefix(name)
        .and_then(|rest| rest.strip_prefix('('))
        .and_then(|rest| rest.strip_suffix(')'))?;
    let parts = split_top_level_args(call);
    if parts.len() != N {
        return None;
    }
    Some(std::array::from_fn(|index| parts[index].trim()))
}

fn strip_wrapping_machine_parens(input: &str) -> &str {
    let mut current = input.trim();
    loop {
        if !(current.starts_with('(') && current.ends_with(')')) {
            return current;
        }
        let mut depth = 0usize;
        let mut wraps = true;
        for (index, ch) in current.char_indices() {
            match ch {
                '(' => depth += 1,
                ')' => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 && index != current.len() - 1 {
                        wraps = false;
                        break;
                    }
                }
                _ => {}
            }
        }
        if wraps {
            current = current[1..current.len() - 1].trim();
        } else {
            return current;
        }
    }
}

fn split_top_level_machine<'a>(input: &'a str, needle: &str) -> Option<(&'a str, &'a str)> {
    let mut depth = 0usize;
    let bytes = input.as_bytes();
    let needle_bytes = needle.as_bytes();
    let mut index = 0usize;
    while index + needle_bytes.len() <= bytes.len() {
        match bytes[index] as char {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            _ => {}
        }
        if depth == 0 && bytes[index..].starts_with(needle_bytes) {
            let left = &input[..index];
            let right = &input[index + needle.len()..];
            return Some((left, right));
        }
        index += 1;
    }
    None
}

fn split_top_level_args(input: &str) -> Vec<&str> {
    let mut depth = 0usize;
    let mut start = 0usize;
    let mut parts = Vec::new();
    for (index, ch) in input.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                parts.push(input[start..index].trim());
                start = index + 1;
            }
            _ => {}
        }
    }
    parts.push(input[start..].trim());
    parts
}

pub fn check_machine_outcome<M: VerifiedMachine>(request_id: &str) -> CheckOutcome {
    let property = primary_property::<M>();
    check_machine_outcome_for_property::<M>(request_id, property.property_id)
}

pub fn check_machine_outcome_for_property<M: VerifiedMachine>(
    request_id: &str,
    property_id: &str,
) -> CheckOutcome {
    let result = check_machine_property::<M>(property_id);
    let run_id = format!(
        "run-{}",
        stable_hash_hex(&(request_id.to_string() + M::model_id() + result.property_id))
            .replace("sha256:", "")
    );
    let source_hash = stable_hash_hex(M::model_id());
    let contract_hash = stable_hash_hex(&(M::model_id().to_string() + result.property_id));
    let manifest = RunManifest {
        request_id: request_id.to_string(),
        run_id: run_id.clone(),
        schema_version: "1.0.0".to_string(),
        source_hash,
        contract_hash,
        engine_version: env!("CARGO_PKG_VERSION").to_string(),
        backend_name: BackendKind::Explicit,
        backend_version: env!("CARGO_PKG_VERSION").to_string(),
        seed: None,
    };

    let (status, reason_code, summary, trace) = match result.status {
        ModelingRunStatus::Pass => (
            RunStatus::Pass,
            Some("COMPLETE_SPACE_EXHAUSTED".to_string()),
            "no violating state found in the reachable state space".to_string(),
            None,
        ),
        ModelingRunStatus::Fail => {
            let trace = build_evidence_trace::<M>(request_id, &result);
            (
                RunStatus::Fail,
                Some("PROPERTY_VIOLATED".to_string()),
                "violating state discovered in reachable state space".to_string(),
                Some(trace),
            )
        }
    };

    let evidence_id = trace.as_ref().map(|item| item.evidence_id.clone());
    CheckOutcome::Completed(ExplicitRunResult {
        manifest,
        status,
        assurance_level: AssuranceLevel::Complete,
        property_result: PropertyResult {
            property_id: result.property_id.to_string(),
            property_kind: find_property::<M>(result.property_id).property_kind,
            status,
            assurance_level: AssuranceLevel::Complete,
            reason_code,
            unknown_reason: None,
            terminal_state_id: trace
                .as_ref()
                .and_then(|item| item.steps.last().map(|step| step.to_state_id.clone())),
            evidence_id,
            summary,
        },
        explored_states: result.explored_states,
        explored_transitions: result.explored_transitions,
        trace,
    })
}

pub fn check_machine_outcomes<M: VerifiedMachine>(request_id: &str) -> Vec<ExplicitRunResult> {
    property_ids::<M>()
        .into_iter()
        .filter_map(|property_id| {
            match check_machine_outcome_for_property::<M>(request_id, property_id) {
                CheckOutcome::Completed(result) => Some(result),
                CheckOutcome::Errored(_) => None,
            }
        })
        .collect()
}

pub fn check_machine_with_adapter<M: VerifiedMachine>(
    request_id: &str,
    property_id: Option<&str>,
    adapter: &AdapterConfig,
) -> Result<CheckOutcome, String> {
    if matches!(adapter, AdapterConfig::Explicit) {
        return Ok(match property_id {
            Some(property_id) => check_machine_outcome_for_property::<M>(request_id, property_id),
            None => check_machine_outcome::<M>(request_id),
        });
    }
    let model = lower_machine_model::<M>()?;
    let snapshot = snapshot_model(&model);
    let property_id = property_id
        .map(str::to_string)
        .or_else(|| {
            model
                .properties
                .first()
                .map(|property| property.property_id.clone())
        })
        .ok_or_else(|| format!("model `{}` has no properties", M::model_id()))?;
    let mut plan = RunPlan::default();
    plan.manifest = RunManifest {
        request_id: request_id.to_string(),
        run_id: format!(
            "run-{}",
            stable_hash_hex(&(request_id.to_string() + &property_id)).replace("sha256:", "")
        ),
        schema_version: "1.0.0".to_string(),
        source_hash: stable_hash_hex(M::model_id()),
        contract_hash: snapshot.contract_hash,
        engine_version: env!("CARGO_PKG_VERSION").to_string(),
        backend_name: backend_kind_for_adapter(adapter),
        backend_version: backend_version_for_adapter(adapter),
        seed: None,
    };
    plan.property_selection = PropertySelection::ExactlyOne(property_id);
    run_with_adapter(&model, &plan, adapter).map(|normalized| normalized.outcome)
}

fn backend_kind_for_adapter(adapter: &AdapterConfig) -> BackendKind {
    match adapter {
        AdapterConfig::Explicit => BackendKind::Explicit,
        AdapterConfig::MockBmc | AdapterConfig::Command { .. } => BackendKind::MockBmc,
        AdapterConfig::SmtCvc5 { .. } => BackendKind::SmtCvc5,
        AdapterConfig::SatVarisat => BackendKind::SatVarisat,
    }
}

fn backend_version_for_adapter(adapter: &AdapterConfig) -> String {
    match adapter {
        AdapterConfig::Explicit | AdapterConfig::MockBmc => env!("CARGO_PKG_VERSION").to_string(),
        AdapterConfig::SatVarisat => env!("CARGO_PKG_VERSION").to_string(),
        AdapterConfig::SmtCvc5 { .. } | AdapterConfig::Command { .. } => "external".to_string(),
    }
}

pub fn replay_machine_actions<M: VerifiedMachine>(
    property_id: Option<&str>,
    action_ids: &[String],
    focus_action_id: Option<&str>,
) -> Result<(BTreeMap<String, Value>, &'static str, Option<bool>), String> {
    let property = property_id
        .map(find_property::<M>)
        .unwrap_or_else(primary_property::<M>);
    let mut states = M::init_states();
    let mut state = states
        .drain(..)
        .next()
        .ok_or_else(|| "ModelSpec::init_states must return at least one state".to_string())?;
    for action_id in action_ids {
        let action = M::Action::all()
            .into_iter()
            .find(|candidate| candidate.action_id() == *action_id)
            .ok_or_else(|| format!("unknown action `{action_id}`"))?;
        let mut next_states = M::step(&state, &action);
        state = next_states
            .drain(..)
            .next()
            .ok_or_else(|| format!("action `{action_id}` was not enabled during replay"))?;
    }
    let focus_action_enabled = focus_action_id.map(|target| {
        M::Action::all()
            .into_iter()
            .find(|action| action.action_id() == target)
            .map(|action| !M::step(&state, &action).is_empty())
            .unwrap_or(false)
    });
    Ok((state.snapshot(), property.property_id, focus_action_enabled))
}

fn build_trace<M: VerifiedMachine>(
    nodes: &[ModelingNode<M::State, M::Action>],
    end_index: usize,
) -> Vec<ModelingTraceStep<M::State, M::Action>> {
    let mut indices = Vec::new();
    let mut cursor = Some(end_index);
    while let Some(index) = cursor {
        indices.push(index);
        cursor = nodes[index].parent;
    }
    indices.reverse();

    let mut trace = Vec::new();
    for (step_index, pair) in indices.windows(2).enumerate() {
        let before = &nodes[pair[0]];
        let after = &nodes[pair[1]];
        trace.push(ModelingTraceStep {
            index: step_index,
            action: after
                .via_action
                .clone()
                .expect("non-root node must have an action"),
            state_before: before.state.clone(),
            state_after: after.state.clone(),
        });
    }
    trace
}

#[derive(Debug, Clone)]
struct ModelingExploration<S, A> {
    nodes: Vec<ModelingNode<S, A>>,
    edges: Vec<ModelingEdge<S, A>>,
    explored_transitions: usize,
    visited_states: usize,
    failure_index: Option<usize>,
}

fn explore_machine<M: VerifiedMachine>(
    holds: fn(&M::State) -> bool,
) -> ModelingExploration<M::State, M::Action> {
    let actions = M::Action::all();
    let init_states = M::init_states();
    assert!(
        !init_states.is_empty(),
        "VerifiedMachine::init_states must return at least one state"
    );

    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    let mut frontier = VecDeque::new();
    let mut visited = HashSet::new();
    let mut explored_transitions = 0usize;

    for state in init_states {
        if visited.insert(state.clone()) {
            let index = nodes.len();
            nodes.push(ModelingNode {
                state,
                parent: None,
                via_action: None,
                depth: 0,
            });
            frontier.push_back(index);
        }
    }

    let mut failure_index = None;
    while let Some(node_index) = frontier.pop_front() {
        let node = nodes[node_index].clone();
        if !holds(&node.state) {
            failure_index = Some(node_index);
            break;
        }

        for action in &actions {
            let next_states = M::step(&node.state, action);
            explored_transitions += 1;
            for next_state in next_states {
                let prior_state = next_state.clone();
                let to_index = if visited.insert(next_state.clone()) {
                    let child_index = nodes.len();
                    nodes.push(ModelingNode {
                        state: next_state,
                        parent: Some(node_index),
                        via_action: Some(action.clone()),
                        depth: node.depth + 1,
                    });
                    frontier.push_back(child_index);
                    child_index
                } else {
                    nodes
                        .iter()
                        .position(|item| item.state == prior_state)
                        .expect("visited state must exist in node list")
                };
                edges.push(ModelingEdge {
                    from_index: node_index,
                    to_index,
                    action: action.clone(),
                    state_before: node.state.clone(),
                    state_after: nodes[to_index].state.clone(),
                });
            }
        }
    }

    ModelingExploration {
        nodes,
        edges,
        explored_transitions,
        visited_states: visited.len(),
        failure_index,
    }
}

fn modeling_result_from_failure<M: VerifiedMachine>(
    exploration: &ModelingExploration<M::State, M::Action>,
    failure_index: usize,
    property_id: &'static str,
) -> ModelingCheckResult<M::State, M::Action> {
    ModelingCheckResult {
        model_id: M::model_id(),
        property_id,
        status: ModelingRunStatus::Fail,
        explored_states: exploration.visited_states,
        explored_transitions: exploration.explored_transitions,
        trace: build_trace::<M>(&exploration.nodes, failure_index),
    }
}

fn build_evidence_trace<M: VerifiedMachine>(
    request_id: &str,
    result: &ModelingCheckResult<M::State, M::Action>,
) -> EvidenceTrace {
    let run_id = format!(
        "run-{}",
        stable_hash_hex(&(request_id.to_string() + M::model_id() + result.property_id))
            .replace("sha256:", "")
    );
    let evidence_id = format!("ev-{run_id}");
    let steps = result
        .trace
        .iter()
        .enumerate()
        .map(|(index, step)| TraceStep {
            index,
            from_state_id: if index == 0 {
                "s-init".to_string()
            } else {
                format!("s-{index}")
            },
            action_id: Some(step.action.action_id()),
            action_label: Some(step.action.action_label()),
            to_state_id: format!("s-{}", index + 1),
            depth: (index + 1) as u32,
            state_before: step.state_before.snapshot(),
            state_after: step.state_after.snapshot(),
            note: None,
        })
        .collect::<Vec<_>>();
    let trace_hash = stable_hash_hex(
        &steps
            .iter()
            .map(|step| format!("{:?}{:?}", step.action_id, step.state_after))
            .collect::<String>(),
    );
    EvidenceTrace {
        schema_version: "1.0.0".to_string(),
        evidence_id,
        run_id,
        property_id: result.property_id.to_string(),
        evidence_kind: EvidenceKind::Trace,
        assurance_level: AssuranceLevel::Complete,
        trace_hash,
        steps,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        action_descriptors, build_machine_test_vectors, check_machine, check_machine_outcome,
        check_machine_outcomes, collect_machine_coverage, contains, explain_machine, iff, implies,
        insert, is_empty, lower_machine_expr, lower_machine_model, machine_capability_report,
        machine_transition_ir, property_ids, remove, state_field_descriptors,
        transition_descriptors, xor, FiniteEnumSet, ModelingRunStatus, ModelingState, StateSpec,
    };
    use crate::{
        engine::{CheckOutcome, PropertySelection, RunManifest, RunPlan, RunStatus},
        ir::{BinaryOp, ExprIr, FieldType, Value},
        solver::{run_with_adapter, AdapterConfig},
        valid_actions, valid_state,
    };

    valid_state! {
        struct State {
            x: u8 [range = "0..=3"],
            locked: bool,
        }
    }

    valid_actions! {
        enum Action {
            Inc => "INC" [reads = ["x", "locked"], writes = ["x"]],
            Lock => "LOCK" [reads = ["locked"], writes = ["locked"]],
            Unlock => "UNLOCK" [reads = ["locked"], writes = ["locked"]],
        }
    }

    crate::valid_model! {
        model CounterModel<State, Action>;
        init [State {
            x: 0,
            locked: false,
        }];
        step |state, action| {
            match action {
                Action::Inc if !state.locked && state.x < 3 => vec![State {
                    x: state.x + 1,
                    locked: state.locked,
                }],
                Action::Lock => vec![State {
                    x: state.x,
                    locked: true,
                }],
                Action::Unlock => vec![State {
                    x: state.x,
                    locked: false,
                }],
                _ => Vec::new(),
            }
        }
        properties {
            invariant P_RANGE |state| state.x <= 3;
            invariant P_LOCKED_RANGE |state| !state.locked || state.x <= 3;
        }
    }

    crate::valid_model! {
        model FailingCounterModel<State, Action>;
        init [State {
            x: 0,
            locked: false,
        }];
        step |state, action| {
            match action {
                Action::Inc if !state.locked && state.x < 3 => vec![State {
                    x: state.x + 1,
                    locked: state.locked,
                }],
                Action::Lock => vec![State {
                    x: state.x,
                    locked: true,
                }],
                Action::Unlock => vec![State {
                    x: state.x,
                    locked: false,
                }],
                _ => Vec::new(),
            }
        }
        properties {
            invariant P_FAIL |state| state.x <= 1;
        }
    }

    #[test]
    fn rust_native_model_can_pass() {
        let result = check_machine::<CounterModel>();
        assert_eq!(result.status, ModelingRunStatus::Pass);
        assert!(result.trace.is_empty());
    }

    #[test]
    fn rust_native_model_can_fail_with_shortest_trace() {
        let result = check_machine::<FailingCounterModel>();
        assert_eq!(result.status, ModelingRunStatus::Fail);
        assert_eq!(result.trace.len(), 2);
    }

    #[test]
    fn modeling_check_can_produce_common_outcome() {
        let outcome = check_machine_outcome::<FailingCounterModel>("req-modeling");
        match outcome {
            CheckOutcome::Completed(result) => {
                assert_eq!(result.status, crate::engine::RunStatus::Fail);
                assert!(result.trace.is_some());
                assert_eq!(result.property_result.property_id, "P_FAIL");
            }
            CheckOutcome::Errored(error) => panic!("unexpected error: {:?}", error.diagnostics),
        }
    }

    #[test]
    fn modeling_check_can_produce_coverage() {
        let report = collect_machine_coverage::<CounterModel>();
        assert_eq!(report.model_id, "CounterModel");
        assert!(report.transition_coverage_percent >= 66);
        assert!(report.visited_state_count >= 4);
        assert!(report.guard_true_counts.contains_key("INC"));
        assert!(report.path_tag_counts.contains_key("state_gate_path"));
    }

    #[test]
    fn modeling_spec_exposes_multiple_properties() {
        assert_eq!(
            property_ids::<CounterModel>(),
            vec!["P_RANGE", "P_LOCKED_RANGE"]
        );
        let outcomes = check_machine_outcomes::<CounterModel>("req-modeling-all");
        assert_eq!(outcomes.len(), 2);
        assert!(outcomes
            .iter()
            .all(|outcome| outcome.status == crate::engine::RunStatus::Pass));
    }

    #[test]
    fn modeling_spec_exposes_state_and_action_metadata() {
        let fields = state_field_descriptors::<State>();
        assert_eq!(fields[0].name, "x");
        assert_eq!(fields[0].range, Some("0..=3"));
        let actions = action_descriptors::<Action>();
        assert_eq!(actions[0].action_id, "INC");
        assert_eq!(actions[0].reads, &["x", "locked"]);
        assert_eq!(actions[0].writes, &["x"]);
    }

    #[derive(Debug, Clone, PartialEq, Eq, Hash)]
    struct AttachedState {
        x: u8,
        locked: bool,
    }

    crate::valid_state_spec! {
        AttachedState {
            x: u8 [range = "0..=3"],
            locked: bool,
        }
    }

    #[derive(Debug, Clone, PartialEq, Eq, Hash)]
    enum AttachedAction {
        Inc,
        Lock,
    }

    crate::valid_action_spec! {
        AttachedAction {
            Inc => "INC" [reads = ["x"], writes = ["x"]],
            Lock => "LOCK" [reads = ["locked"], writes = ["locked"]],
        }
    }

    #[test]
    fn spec_macros_attach_traits_to_existing_types() {
        let snapshot = AttachedState { x: 2, locked: true }.snapshot();
        assert_eq!(snapshot.get("x"), Some(&crate::ir::Value::UInt(2)));
        let fields = state_field_descriptors::<AttachedState>();
        assert_eq!(fields[0].range, Some("0..=3"));
        let actions = action_descriptors::<AttachedAction>();
        assert_eq!(actions[0].action_id, "INC");
        assert_eq!(actions[1].writes, &["locked"]);
    }

    #[derive(crate::ValidState, Debug, Clone, PartialEq, Eq, Hash)]
    struct DerivedState {
        #[valid(range = "0..=3")]
        x: u8,
        locked: bool,
    }

    #[derive(crate::ValidAction, Debug, Clone, PartialEq, Eq, Hash)]
    enum DerivedAction {
        #[valid(action_id = "INC", reads = ["x"], writes = ["x"])]
        Inc,
        #[valid(action_id = "LOCK", reads = ["locked"], writes = ["locked"])]
        Lock,
    }

    #[derive(crate::ValidEnum, Debug, Clone, Copy, PartialEq, Eq, Hash)]
    enum ReviewStage {
        Draft,
        Approved,
    }

    #[derive(crate::ValidState, Debug, Clone, PartialEq, Eq, Hash)]
    struct EnumState {
        #[valid(enum)]
        review_stage: ReviewStage,
        active: bool,
    }

    #[derive(crate::ValidState, Debug, Clone, PartialEq, Eq, Hash)]
    struct OptionalEnumState {
        #[valid(enum)]
        review_stage: Option<ReviewStage>,
        active: bool,
    }

    #[test]
    fn derive_macros_attach_model_traits() {
        let snapshot = DerivedState {
            x: 1,
            locked: false,
        }
        .snapshot();
        assert_eq!(snapshot.get("x"), Some(&crate::ir::Value::UInt(1)));
        let fields = state_field_descriptors::<DerivedState>();
        assert_eq!(fields[0].range, Some("0..=3"));
        let actions = action_descriptors::<DerivedAction>();
        assert_eq!(actions[0].action_id, "INC");
        assert_eq!(actions[0].reads, &["x"]);
        assert_eq!(actions[1].writes, &["locked"]);
    }

    #[test]
    fn valid_enum_fields_expose_variants_and_snapshot_labels() {
        let snapshot = EnumState {
            review_stage: ReviewStage::Approved,
            active: true,
        }
        .snapshot();
        assert_eq!(
            snapshot.get("review_stage"),
            Some(&crate::ir::Value::EnumVariant {
                label: "Approved".to_string(),
                index: 1,
            })
        );
        let fields = state_field_descriptors::<EnumState>();
        assert_eq!(
            fields[0].variants.as_ref().unwrap(),
            &vec!["Draft", "Approved"]
        );
    }

    #[test]
    fn optional_enum_fields_expose_none_and_some_variants() {
        let snapshot = OptionalEnumState {
            review_stage: Some(ReviewStage::Approved),
            active: false,
        }
        .snapshot();
        assert_eq!(
            snapshot.get("review_stage"),
            Some(&crate::ir::Value::EnumVariant {
                label: "Some(Approved)".to_string(),
                index: 2,
            })
        );
        let fields = state_field_descriptors::<OptionalEnumState>();
        assert_eq!(
            fields[0].variants.as_ref().unwrap(),
            &vec!["None", "Some(Draft)", "Some(Approved)"]
        );
    }

    valid_state! {
        struct AccessState {
            attached: bool,
            allowed: bool,
        }
    }

    valid_actions! {
        enum AccessAction {
            AttachPolicy => "ATTACH_POLICY" [reads = ["attached"], writes = ["attached"]],
            EvaluateRead => "EVAL_READ" [reads = ["attached"], writes = ["allowed"]],
        }
    }

    crate::valid_model! {
        model AccessModel<AccessState, AccessAction>;
        init [AccessState {
            attached: false,
            allowed: false,
        }];
        transitions {
            transition AttachPolicy [tags = ["boundary_path"]] when |state| !state.attached => [AccessState {
                attached: true,
                allowed: state.allowed,
            }];
            transition EvaluateRead [tags = ["allow_path", "boundary_path"]] when |state| state.attached && !state.allowed => [AccessState {
                attached: state.attached,
                allowed: true,
            }];
        }
        properties {
            invariant P_ACCESS_REQUIRES_ATTACHMENT |state| !state.allowed || state.attached;
        }
    }

    #[test]
    fn declarative_transition_models_expose_transition_metadata() {
        let transitions = transition_descriptors::<AccessModel>();
        assert_eq!(transitions.len(), 2);
        assert_eq!(transitions[0].action_id, "ATTACH_POLICY");
        assert_eq!(transitions[0].reads, &["attached"]);
        assert_eq!(transitions[0].path_tags, &["boundary_path"]);
        assert_eq!(transitions[1].writes, &["allowed"]);
        assert_eq!(transitions[1].path_tags, &["allow_path", "boundary_path"]);
        assert!(transitions[1].guard.contains("state.attached"));
    }

    #[test]
    fn machine_transition_ir_normalizes_transition_metadata() {
        let transitions = machine_transition_ir::<AccessModel>();
        assert_eq!(transitions.len(), 2);
        assert_eq!(transitions[0].action_id, "ATTACH_POLICY");
        assert_eq!(transitions[0].guard, Some("!state.attached"));
        assert_eq!(transitions[0].path_tags, vec!["boundary_path"]);
        assert_eq!(transitions[0].updates[0].field, "attached");
        assert_eq!(transitions[0].updates[0].expr, Some("true"));
        let implicit = machine_transition_ir::<CounterModel>();
        assert_eq!(implicit.len(), 3);
        assert!(implicit.iter().all(|transition| transition.guard.is_none()));
        assert!(implicit
            .iter()
            .all(|transition| transition.path_tags.is_empty()));
    }

    #[test]
    fn declarative_model_can_lower_to_machine_ir() {
        let model = lower_machine_model::<AccessModel>().expect("machine lowering should work");
        assert_eq!(model.model_id, "AccessModel");
        assert_eq!(model.actions.len(), 2);
        assert_eq!(model.properties.len(), 1);
        assert!(matches!(
            model.actions[1].guard,
            ExprIr::Binary {
                op: BinaryOp::And,
                ..
            }
        ));
        assert_eq!(model.actions[0].updates[0].field, "attached");
    }

    crate::valid_model! {
        model ReviewStageModel<EnumState, AttachedAction>;
        init [EnumState {
            review_stage: ReviewStage::Draft,
            active: false,
        }];
        transitions {
            transition Inc [tags = ["approval_path"]] when |state| state.review_stage == ReviewStage::Draft => [EnumState {
                review_stage: ReviewStage::Approved,
                active: state.active,
            }];
            transition Lock [tags = ["deny_path"]] when |state| state.review_stage == ReviewStage::Approved => [EnumState {
                review_stage: state.review_stage,
                active: true,
            }];
        }
        properties {
            invariant P_ACTIVE_REQUIRES_APPROVAL |state| state.active == false || state.review_stage == ReviewStage::Approved;
        }
    }

    #[test]
    fn declarative_model_can_lower_enum_literals() {
        let model = lower_machine_model::<ReviewStageModel>().expect("enum lowering should work");
        assert!(matches!(model.state_fields[0].ty, FieldType::Enum { .. }));
        assert!(matches!(
            model.actions[0].guard,
            ExprIr::Binary {
                op: BinaryOp::Equal,
                ..
            }
        ));
        assert!(matches!(
            model.actions[0].updates[0].value,
            ExprIr::Literal(Value::EnumVariant { .. })
        ));
    }

    crate::valid_model! {
        model OptionalReviewStageModel<OptionalEnumState, AttachedAction>;
        init [OptionalEnumState {
            review_stage: None,
            active: false,
        }];
        transitions {
            transition Inc [tags = ["approval_path"]] when |state| state.review_stage == None => [OptionalEnumState {
                review_stage: Some(ReviewStage::Approved),
                active: state.active,
            }];
            transition Lock [tags = ["deny_path"]] when |state| state.review_stage == Some(ReviewStage::Approved) => [OptionalEnumState {
                review_stage: state.review_stage,
                active: true,
            }];
        }
        properties {
            invariant P_ACTIVE_REQUIRES_OPTIONAL_APPROVAL |state| state.active == false || state.review_stage == Some(ReviewStage::Approved);
        }
    }

    #[test]
    fn declarative_model_can_lower_optional_enum_literals() {
        let model = lower_machine_model::<OptionalReviewStageModel>()
            .expect("optional enum lowering should work");
        assert!(matches!(model.state_fields[0].ty, FieldType::Enum { .. }));
        assert!(matches!(
            model.actions[0].guard,
            ExprIr::Binary {
                op: BinaryOp::Equal,
                ..
            }
        ));
        assert!(matches!(
            model.actions[0].updates[0].value,
            ExprIr::Literal(Value::EnumVariant { .. })
        ));
    }

    #[test]
    fn lower_machine_expr_supports_extended_numeric_ops() {
        let expr = lower_machine_expr("state.risk_score - 1 > 0 && state.risk_score >= 1 && state.risk_score < 3 && state.risk_score % 2 == 1 && state.manager_approved != false")
            .expect("extended machine expr should lower");
        let debug = format!("{expr:?}");
        assert!(debug.contains("Sub"));
        assert!(debug.contains("Mod"));
        assert!(debug.contains("GreaterThan"));
        assert!(debug.contains("GreaterThanOrEqual"));
        assert!(debug.contains("LessThan"));
        assert!(debug.contains("NotEqual"));
    }

    #[allow(dead_code)]
    #[derive(Debug, Clone, PartialEq, Eq, Hash, crate::ValidEnum)]
    enum MacroReviewStage {
        Approved,
        Rejected,
    }

    valid_state! {
        struct MacroEnumState {
            review_stage: Option<MacroReviewStage> [enum],
            active: bool,
        }
    }

    #[test]
    fn valid_state_macro_supports_optional_enum_metadata() {
        let fields = <MacroEnumState as StateSpec>::state_fields();
        assert_eq!(
            fields[0].variants.as_ref().expect("enum variants"),
            &vec![
                "None".to_string(),
                "Some(Approved)".to_string(),
                "Some(Rejected)".to_string()
            ]
        );
    }

    #[test]
    fn opaque_step_model_does_not_lower_to_machine_ir() {
        let error = lower_machine_model::<CounterModel>().unwrap_err();
        assert!(error.contains("declarative transitions"));
    }

    #[test]
    fn step_models_report_explicit_only_capabilities() {
        let report = machine_capability_report::<CounterModel>();
        assert!(report.explicit_ready);
        assert!(!report.ir_ready);
        assert!(!report.solver_ready);
        assert!(report.reasons.contains(&"opaque_step_closure".to_string()));
        assert!(report
            .reasons
            .contains(&"missing_declarative_transitions".to_string()));
    }

    #[test]
    fn declarative_models_report_solver_ready_capabilities() {
        let report = machine_capability_report::<AccessModel>();
        assert!(report.explicit_ready);
        assert!(report.ir_ready);
        assert!(report.solver_ready);
        assert!(report.reasons.is_empty());
    }

    #[test]
    fn modeling_check_can_produce_explain() {
        let explain =
            explain_machine::<FailingCounterModel>("req-explain").expect("explain should exist");
        assert_eq!(explain.property_id, "P_FAIL");
        assert!(!explain.candidate_causes.is_empty());
        assert!(explain.confidence > 0.4);
    }

    #[test]
    fn modeling_check_can_produce_test_vectors() {
        let counterexample_vectors = build_machine_test_vectors::<FailingCounterModel>();
        assert_eq!(counterexample_vectors.len(), 1);
        assert_eq!(counterexample_vectors[0].strategy, "counterexample");

        let witness_vectors = build_machine_test_vectors::<CounterModel>();
        assert!(!witness_vectors.is_empty());
        assert!(witness_vectors
            .iter()
            .all(|vector| vector.strategy == "witness"));
    }

    #[allow(dead_code)]
    #[derive(crate::ValidEnum, Debug, Clone, Copy, PartialEq, Eq, Hash)]
    enum Role {
        Reader,
        Admin,
    }

    valid_state! {
        struct RoleSetState {
            roles: FiniteEnumSet<Role> [set],
            approved: bool,
        }
    }

    valid_actions! {
        enum RoleSetAction {
            GrantAdmin => "GRANT_ADMIN" [reads = ["roles", "approved"], writes = ["roles", "approved"]],
            RevokeAdmin => "REVOKE_ADMIN" [reads = ["roles", "approved"], writes = ["roles"]],
        }
    }

    crate::valid_model! {
        model RoleSetModel<RoleSetState, RoleSetAction>;
        init [RoleSetState {
            roles: FiniteEnumSet::empty(),
            approved: false,
        }];
        transitions {
            transition GrantAdmin [tags = ["approval_path", "allow_path"]] when |state| is_empty(state.roles) => [RoleSetState {
                roles: insert(state.roles, Role::Admin),
                approved: true,
            }];
            transition RevokeAdmin [tags = ["recovery_path"]] when |state| contains(state.roles, Role::Admin) => [RoleSetState {
                roles: remove(state.roles, Role::Admin),
                approved: state.approved,
            }];
        }
        properties {
            invariant P_ADMIN_IMPLIES_APPROVED |state| implies(contains(state.roles, Role::Admin), state.approved);
            invariant P_APPROVED_IFF_NOT_EMPTY |state| iff(state.approved, xor(is_empty(state.roles), true));
        }
    }

    valid_state! {
        struct BranchState {
            x: u8 [range = "0..=3"],
            even: bool,
        }
    }

    valid_actions! {
        enum BranchAction {
            Step => "STEP" [reads = ["x", "even"], writes = ["x", "even"]],
        }
    }

    crate::valid_model! {
        model BranchModel<BranchState, BranchAction>;
        init [BranchState {
            x: 0,
            even: true,
        }];
        transitions {
            on Step {
                [tags = ["even_path"]] when |state| state.x < 3 && (state.x + 1) % 2 == 0 => [BranchState {
                    x: state.x + 1,
                    even: true,
                }];
                [tags = ["odd_path"]] when |state| state.x < 3 && (state.x + 1) % 2 != 0 => [BranchState {
                    x: state.x + 1,
                    even: false,
                }];
            }
        }
        properties {
            invariant P_BRANCH_BOUND |state| state.x <= 3;
        }
    }

    #[test]
    fn declarative_model_can_lower_finite_sets_and_logical_helpers() {
        let fields = state_field_descriptors::<RoleSetState>();
        assert!(fields[0].is_set);
        assert_eq!(
            fields[0].variants.as_ref().unwrap(),
            &vec!["Reader", "Admin"]
        );

        let lowered = lower_machine_model::<RoleSetModel>().expect("set lowering should work");
        assert!(matches!(
            lowered.state_fields[0].ty,
            FieldType::EnumSet { .. }
        ));
        assert!(matches!(
            lowered.actions[0].guard,
            ExprIr::Unary { .. } | ExprIr::Binary { .. }
        ));
        assert!(matches!(
            lowered.actions[0].updates[0].value,
            ExprIr::Binary {
                op: BinaryOp::SetInsert,
                ..
            }
        ));
        let property_debug = format!("{:?}", lowered.properties[0].expr);
        assert!(property_debug.contains("Or"));
        assert!(property_debug.contains("Not"));
    }

    #[test]
    fn duplicate_action_transitions_are_explored_and_lowered() {
        let result = check_machine::<BranchModel>();
        assert_eq!(result.status, ModelingRunStatus::Pass);
        assert_eq!(result.explored_states, 4);

        let lowered = lower_machine_model::<BranchModel>().expect("branch model lowers");
        assert_eq!(lowered.actions.len(), 2);
        let mut plan = RunPlan::default();
        plan.manifest = RunManifest {
            request_id: "req-branch".to_string(),
            run_id: "run-branch".to_string(),
            schema_version: "1.0.0".to_string(),
            source_hash: "sha256:test".to_string(),
            contract_hash: "sha256:test".to_string(),
            engine_version: env!("CARGO_PKG_VERSION").to_string(),
            backend_name: crate::engine::BackendKind::Explicit,
            backend_version: env!("CARGO_PKG_VERSION").to_string(),
            seed: None,
        };
        plan.property_selection = PropertySelection::ExactlyOne("P_BRANCH_BOUND".to_string());
        plan.detect_deadlocks = false;
        let outcome = run_with_adapter(&lowered, &plan, &AdapterConfig::Explicit)
            .expect("explicit adapter should run");
        match outcome.outcome {
            CheckOutcome::Completed(result) => {
                assert_eq!(result.status, RunStatus::Pass);
                assert_eq!(result.explored_states, 4);
            }
            CheckOutcome::Errored(error) => panic!("unexpected error: {:?}", error.diagnostics),
        }
    }
}
