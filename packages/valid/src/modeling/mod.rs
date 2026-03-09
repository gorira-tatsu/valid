//! Rust-based modeling contracts.
//!
//! This module exposes only generic system-side contracts. Concrete domain
//! models belong in user code, examples, or tests rather than inside `src/`.

#[cfg(any(debug_assertions, feature = "verification-runtime"))]
use std::collections::{HashSet, VecDeque};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Debug,
    hash::Hash,
    marker::PhantomData,
    sync::{Mutex, OnceLock},
};

#[cfg(any(debug_assertions, feature = "verification-runtime"))]
use crate::ir::{
    ActionIr, FieldType, InitAssignment, ModelIr, PropertyIr, SourceSpan, StateField, UpdateIr,
};
use crate::ir::{BinaryOp, ExprIr, UnaryOp, Value};
#[cfg(any(debug_assertions, feature = "verification-runtime"))]
use crate::{
    api::{ExplainCandidateCause, ExplainResponse},
    contract::snapshot_model,
    coverage::CoverageReport,
    engine::{
        build_run_manifest, AssuranceLevel, BackendKind, CheckOutcome, ExplicitRunResult,
        PropertyResult, PropertySelection, RunPlan, RunStatus,
    },
    evidence::{EvidenceKind, EvidenceTrace, TraceStep},
    ir::{
        build_path_from_parts, decision_path_tags as ir_decision_path_tags,
        infer_decision_path_tags as ir_infer_decision_path_tags, Path,
    },
    solver::{
        backend_version_for_config as solver_backend_version_for_config, run_with_adapter,
        AdapterConfig,
    },
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FiniteRelation<A, B> {
    bits: u64,
    _marker: PhantomData<(A, B)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FiniteMap<K, V> {
    bits: u64,
    _marker: PhantomData<(K, V)>,
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

impl<A, B> FiniteRelation<A, B>
where
    A: FiniteValueSpec,
    B: FiniteValueSpec,
{
    pub fn empty() -> Self {
        Self {
            bits: 0,
            _marker: PhantomData,
        }
    }

    pub fn bits(self) -> u64 {
        self.bits
    }

    pub fn contains(self, left: A, right: B) -> bool {
        relation_mask::<A, B>(left.variant_index(), right.variant_index())
            .map(|mask| self.bits & mask != 0)
            .unwrap_or(false)
    }

    pub fn insert(self, left: A, right: B) -> Self {
        let bits = relation_mask::<A, B>(left.variant_index(), right.variant_index())
            .map(|mask| self.bits | mask)
            .unwrap_or(self.bits);
        Self {
            bits,
            _marker: PhantomData,
        }
    }

    pub fn remove(self, left: A, right: B) -> Self {
        let bits = relation_mask::<A, B>(left.variant_index(), right.variant_index())
            .map(|mask| self.bits & !mask)
            .unwrap_or(self.bits);
        Self {
            bits,
            _marker: PhantomData,
        }
    }

    pub fn intersects(self, other: Self) -> bool {
        self.bits & other.bits != 0
    }

    pub fn is_empty(self) -> bool {
        self.bits == 0
    }
}

impl<A, B> Default for FiniteRelation<A, B>
where
    A: FiniteValueSpec,
    B: FiniteValueSpec,
{
    fn default() -> Self {
        Self::empty()
    }
}

impl<K, V> FiniteMap<K, V>
where
    K: FiniteValueSpec,
    V: FiniteValueSpec,
{
    pub fn empty() -> Self {
        Self {
            bits: 0,
            _marker: PhantomData,
        }
    }

    pub fn bits(self) -> u64 {
        self.bits
    }

    pub fn contains_key(self, key: K) -> bool {
        map_value_bits::<K, V>(self.bits, key.variant_index()) != 0
    }

    pub fn contains_entry(self, key: K, value: V) -> bool {
        relation_mask::<K, V>(key.variant_index(), value.variant_index())
            .map(|mask| self.bits & mask != 0)
            .unwrap_or(false)
    }

    pub fn put(self, key: K, value: V) -> Self {
        let cleared = clear_relation_group::<K, V>(self.bits, key.variant_index());
        let bits = relation_mask::<K, V>(key.variant_index(), value.variant_index())
            .map(|mask| cleared | mask)
            .unwrap_or(cleared);
        Self {
            bits,
            _marker: PhantomData,
        }
    }

    pub fn remove(self, key: K) -> Self {
        Self {
            bits: clear_relation_group::<K, V>(self.bits, key.variant_index()),
            _marker: PhantomData,
        }
    }

    pub fn is_empty(self) -> bool {
        self.bits == 0
    }
}

impl<K, V> Default for FiniteMap<K, V>
where
    K: FiniteValueSpec,
    V: FiniteValueSpec,
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

impl IntoModelValue for String {
    fn into_model_value(self) -> Value {
        Value::String(self)
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

pub trait FiniteRelationSpec: Clone + Debug + Eq + Hash {
    fn left_variant_labels() -> &'static [&'static str];
    fn right_variant_labels() -> &'static [&'static str];
}

pub trait FiniteMapSpec: Clone + Debug + Eq + Hash {
    fn key_variant_labels() -> &'static [&'static str];
    fn value_variant_labels() -> &'static [&'static str];
}

impl<T> FiniteSetSpec for FiniteEnumSet<T>
where
    T: FiniteValueSpec,
{
    fn variant_labels() -> &'static [&'static str] {
        T::variant_labels()
    }
}

impl<A, B> FiniteRelationSpec for FiniteRelation<A, B>
where
    A: FiniteValueSpec,
    B: FiniteValueSpec,
{
    fn left_variant_labels() -> &'static [&'static str] {
        A::variant_labels()
    }

    fn right_variant_labels() -> &'static [&'static str] {
        B::variant_labels()
    }
}

impl<K, V> FiniteMapSpec for FiniteMap<K, V>
where
    K: FiniteValueSpec,
    V: FiniteValueSpec,
{
    fn key_variant_labels() -> &'static [&'static str] {
        K::variant_labels()
    }

    fn value_variant_labels() -> &'static [&'static str] {
        V::variant_labels()
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

impl<A, B> IntoModelValue for FiniteRelation<A, B>
where
    A: FiniteValueSpec,
    B: FiniteValueSpec,
{
    fn into_model_value(self) -> Value {
        Value::UInt(self.bits())
    }
}

impl<K, V> IntoModelValue for FiniteMap<K, V>
where
    K: FiniteValueSpec,
    V: FiniteValueSpec,
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

pub fn len<S>(value: &S) -> u64
where
    S: AsRef<str> + ?Sized,
{
    value.as_ref().chars().count() as u64
}

pub fn str_contains<S>(value: &S, needle: &str) -> bool
where
    S: AsRef<str> + ?Sized,
{
    value.as_ref().contains(needle)
}

pub fn regex_match<S>(value: &S, pattern: &str) -> bool
where
    S: AsRef<str> + ?Sized,
{
    regex_match_cached(value.as_ref(), pattern)
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

fn regex_match_cached(value: &str, pattern: &str) -> bool {
    static CACHE: OnceLock<Mutex<BTreeMap<String, regex::Regex>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(BTreeMap::new()));
    let mut cache = cache
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let compiled = cache.entry(pattern.to_string()).or_insert_with(|| {
        regex::Regex::new(pattern).unwrap_or_else(|error| {
            panic!("invalid regex pattern `{pattern}` in model evaluation: {error}")
        })
    });
    compiled.is_match(value)
}

pub fn is_empty<T>(set: FiniteEnumSet<T>) -> bool
where
    T: FiniteValueSpec,
{
    set.is_empty()
}

pub fn rel_contains<A, B>(relation: FiniteRelation<A, B>, left: A, right: B) -> bool
where
    A: FiniteValueSpec,
    B: FiniteValueSpec,
{
    relation.contains(left, right)
}

pub fn rel_insert<A, B>(relation: FiniteRelation<A, B>, left: A, right: B) -> FiniteRelation<A, B>
where
    A: FiniteValueSpec,
    B: FiniteValueSpec,
{
    relation.insert(left, right)
}

pub fn rel_remove<A, B>(relation: FiniteRelation<A, B>, left: A, right: B) -> FiniteRelation<A, B>
where
    A: FiniteValueSpec,
    B: FiniteValueSpec,
{
    relation.remove(left, right)
}

pub fn rel_intersects<A, B>(left: FiniteRelation<A, B>, right: FiniteRelation<A, B>) -> bool
where
    A: FiniteValueSpec,
    B: FiniteValueSpec,
{
    left.intersects(right)
}

pub fn map_contains_key<K, V>(map: FiniteMap<K, V>, key: K) -> bool
where
    K: FiniteValueSpec,
    V: FiniteValueSpec,
{
    map.contains_key(key)
}

pub fn map_contains_entry<K, V>(map: FiniteMap<K, V>, key: K, value: V) -> bool
where
    K: FiniteValueSpec,
    V: FiniteValueSpec,
{
    map.contains_entry(key, value)
}

pub fn map_put<K, V>(map: FiniteMap<K, V>, key: K, value: V) -> FiniteMap<K, V>
where
    K: FiniteValueSpec,
    V: FiniteValueSpec,
{
    map.put(key, value)
}

pub fn map_remove<K, V>(map: FiniteMap<K, V>, key: K) -> FiniteMap<K, V>
where
    K: FiniteValueSpec,
    V: FiniteValueSpec,
{
    map.remove(key)
}

fn enum_variant_mask(index: u64) -> u64 {
    1u64.checked_shl(index as u32).unwrap_or(0)
}

fn relation_variant_count<A, B>() -> Option<u64>
where
    A: FiniteValueSpec,
    B: FiniteValueSpec,
{
    (A::variant_labels().len() as u64).checked_mul(B::variant_labels().len() as u64)
}

fn relation_mask<A, B>(left_index: u64, right_index: u64) -> Option<u64>
where
    A: FiniteValueSpec,
    B: FiniteValueSpec,
{
    let right_len = B::variant_labels().len() as u64;
    let bit_index = left_index
        .checked_mul(right_len)?
        .checked_add(right_index)?;
    if relation_variant_count::<A, B>()? > 64 {
        return None;
    }
    enum_variant_mask(bit_index).into()
}

fn clear_relation_group<K, V>(bits: u64, key_index: u64) -> u64
where
    K: FiniteValueSpec,
    V: FiniteValueSpec,
{
    let value_len = V::variant_labels().len() as u64;
    let mut cleared = bits;
    for value_index in 0..value_len {
        if let Some(mask) = relation_mask::<K, V>(key_index, value_index) {
            cleared &= !mask;
        }
    }
    cleared
}

fn map_value_bits<K, V>(bits: u64, key_index: u64) -> u64
where
    K: FiniteValueSpec,
    V: FiniteValueSpec,
{
    let value_len = V::variant_labels().len() as u64;
    let mut found = 0u64;
    for value_index in 0..value_len {
        if let Some(mask) = relation_mask::<K, V>(key_index, value_index) {
            if bits & mask != 0 {
                found |= mask;
            }
        }
    }
    found
}

#[cfg(any(debug_assertions, feature = "verification-runtime"))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelingRunStatus {
    Pass,
    Fail,
}

#[cfg(any(debug_assertions, feature = "verification-runtime"))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelingTraceStep<S, A> {
    pub index: usize,
    pub action: A,
    pub state_before: S,
    pub state_after: S,
}

#[cfg(any(debug_assertions, feature = "verification-runtime"))]
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
    pub is_relation: bool,
    pub relation_left_variants: Option<Vec<&'static str>>,
    pub relation_right_variants: Option<Vec<&'static str>>,
    pub is_map: bool,
    pub map_key_variants: Option<Vec<&'static str>>,
    pub map_value_variants: Option<Vec<&'static str>>,
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
    pub role: crate::ir::action::ActionRole,
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

#[cfg(any(debug_assertions, feature = "verification-runtime"))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MachineTransitionUpdateIr {
    pub field: &'static str,
    pub expr: Option<&'static str>,
}

#[cfg(any(debug_assertions, feature = "verification-runtime"))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilityDetail {
    pub reason: String,
    pub migration_hint: Option<String>,
    pub unsupported_features: Vec<String>,
}

impl Default for CapabilityDetail {
    fn default() -> Self {
        Self::ready()
    }
}

impl CapabilityDetail {
    pub fn ready() -> Self {
        Self {
            reason: String::new(),
            migration_hint: None,
            unsupported_features: Vec::new(),
        }
    }

    fn blocked(
        reason: impl Into<String>,
        migration_hint: impl Into<String>,
        unsupported_features: Vec<String>,
    ) -> Self {
        Self {
            reason: reason.into(),
            migration_hint: Some(migration_hint.into()),
            unsupported_features: sorted_unique_strings(unsupported_features),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MachineTransitionIr {
    pub action_variant: &'static str,
    pub action_id: &'static str,
    pub role: crate::ir::action::ActionRole,
    pub guard: Option<&'static str>,
    pub effect: Option<&'static str>,
    pub reads: &'static [&'static str],
    pub writes: &'static [&'static str],
    pub path_tags: Vec<&'static str>,
    pub updates: Vec<MachineTransitionUpdateIr>,
}

#[cfg(any(debug_assertions, feature = "verification-runtime"))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MachineCapabilityReport {
    pub parse_ready: bool,
    pub parse: CapabilityDetail,
    pub explicit_ready: bool,
    pub explicit: CapabilityDetail,
    pub ir_ready: bool,
    pub ir: CapabilityDetail,
    pub solver_ready: bool,
    pub solver: CapabilityDetail,
    pub coverage_ready: bool,
    pub coverage: CapabilityDetail,
    pub explain_ready: bool,
    pub explain: CapabilityDetail,
    pub testgen_ready: bool,
    pub testgen: CapabilityDetail,
    pub machine_ir_error: Option<String>,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CapabilityAssessment {
    codes: Vec<String>,
    detail: CapabilityDetail,
}

impl CapabilityAssessment {
    fn ready() -> Self {
        Self {
            codes: Vec::new(),
            detail: CapabilityDetail::ready(),
        }
    }
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
    ir_infer_decision_path_tags(action_id, reads, writes, guard, effect)
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
    ir_decision_path_tags(explicit_tags, action_id, reads, writes, guard, effect)
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
    pub property_layer: crate::ir::PropertyLayer,
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
        Self::assert_invariant_expr(property_id, expr, holds)
    }

    pub fn assert_invariant(property_id: &'static str, holds: fn(&S) -> bool) -> Self {
        Self::assert_invariant_expr(property_id, None, holds)
    }

    pub fn assert_invariant_expr(
        property_id: &'static str,
        expr: Option<&'static str>,
        holds: fn(&S) -> bool,
    ) -> Self {
        Self {
            property_id,
            property_kind: crate::ir::PropertyKind::Invariant,
            property_layer: crate::ir::PropertyLayer::Assert,
            expr,
            holds,
        }
    }

    pub fn assume_invariant(property_id: &'static str, holds: fn(&S) -> bool) -> Self {
        Self::assume_invariant_expr(property_id, None, holds)
    }

    pub fn assume_invariant_expr(
        property_id: &'static str,
        expr: Option<&'static str>,
        holds: fn(&S) -> bool,
    ) -> Self {
        Self {
            property_id,
            property_kind: crate::ir::PropertyKind::Invariant,
            property_layer: crate::ir::PropertyLayer::Assume,
            expr,
            holds,
        }
    }

    pub fn deadlock_freedom(property_id: &'static str, holds: fn(&S) -> bool) -> Self {
        Self {
            property_id,
            property_kind: crate::ir::PropertyKind::DeadlockFreedom,
            property_layer: crate::ir::PropertyLayer::Assert,
            expr: None,
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

#[cfg(any(debug_assertions, feature = "verification-runtime"))]
pub trait VerifiedMachine: ModelSpec {}

#[cfg(any(debug_assertions, feature = "verification-runtime"))]
impl<T> VerifiedMachine for T where T: ModelSpec {}

#[cfg(debug_assertions)]
fn debug_validation_cache() -> &'static Mutex<HashSet<&'static str>> {
    static CACHE: OnceLock<Mutex<HashSet<&'static str>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashSet::new()))
}

#[cfg(debug_assertions)]
fn run_debug_machine_validation<M: VerifiedMachine>() {
    let mut cache = debug_validation_cache()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if !cache.insert(M::model_id()) {
        return;
    }
    drop(cache);

    let validation = std::panic::catch_unwind(|| {
        let init_states = M::init_states();
        if init_states.is_empty() {
            return Err(format!(
                "ModelSpec::init_states must return at least one state for `{}`",
                M::model_id()
            ));
        }

        let properties = M::properties();
        if properties.is_empty() {
            return Err(format!(
                "ModelSpec::properties must return at least one property for `{}`",
                M::model_id()
            ));
        }

        for state in &init_states {
            let _ = M::enabled_actions(state);
            let _ = M::observe(state);
        }
        Ok::<(), String>(())
    });
    match validation {
        Ok(Ok(())) => {}
        Ok(Err(message)) => {
            eprintln!("debug validation warning: {message}");
        }
        Err(_) => {
            eprintln!(
                "debug validation warning: model `{}` panicked during lightweight validation",
                M::model_id()
            );
        }
    }
}

#[cfg(all(not(debug_assertions), feature = "verification-runtime"))]
fn run_debug_machine_validation<M: VerifiedMachine>() {}

#[cfg(any(debug_assertions, feature = "verification-runtime"))]
fn primary_property<M: ModelSpec>() -> ModelProperty<M::State> {
    run_debug_machine_validation::<M>();
    M::properties()
        .into_iter()
        .next()
        .expect("ModelSpec::properties must return at least one property")
}

#[cfg(any(debug_assertions, feature = "verification-runtime"))]
fn find_property<M: ModelSpec>(property_id: &str) -> ModelProperty<M::State> {
    run_debug_machine_validation::<M>();
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

#[cfg(any(debug_assertions, feature = "verification-runtime"))]
pub fn property_ids<M: ModelSpec>() -> Vec<&'static str> {
    run_debug_machine_validation::<M>();
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

#[cfg(any(debug_assertions, feature = "verification-runtime"))]
pub fn machine_transition_ir<M: ModelSpec>() -> Vec<MachineTransitionIr> {
    let descriptors = M::transitions();
    if !descriptors.is_empty() {
        return descriptors
            .into_iter()
            .map(|descriptor| MachineTransitionIr {
                action_variant: descriptor.action_variant,
                action_id: descriptor.action_id,
                role: descriptor.role,
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
            role: crate::ir::action::ActionRole::Business,
            guard: None,
            effect: None,
            reads: descriptor.reads,
            writes: descriptor.writes,
            path_tags: Vec::new(),
            updates: Vec::new(),
        })
        .collect()
}

#[cfg(any(debug_assertions, feature = "verification-runtime"))]
pub fn machine_capability_report<M: VerifiedMachine>() -> MachineCapabilityReport {
    run_debug_machine_validation::<M>();
    let machine_ir = lower_machine_model::<M>();
    match machine_ir {
        Ok(model) => {
            let solver_subset = machine_solver_capability_assessment(&model);
            MachineCapabilityReport {
                parse_ready: true,
                parse: CapabilityDetail::ready(),
                explicit_ready: true,
                explicit: CapabilityDetail::ready(),
                ir_ready: true,
                ir: CapabilityDetail::ready(),
                solver_ready: solver_subset.codes.is_empty(),
                solver: solver_subset.detail,
                coverage_ready: true,
                coverage: CapabilityDetail::ready(),
                explain_ready: true,
                explain: CapabilityDetail::ready(),
                testgen_ready: true,
                testgen: CapabilityDetail::ready(),
                machine_ir_error: None,
                reasons: solver_subset.codes,
            }
        }
        Err(error) => {
            let ir = machine_ir_capability_assessment::<M>(&error);
            let solver = solver_capability_blocked_by_ir(&ir.detail);
            MachineCapabilityReport {
                parse_ready: true,
                parse: CapabilityDetail::ready(),
                explicit_ready: true,
                explicit: CapabilityDetail::ready(),
                ir_ready: false,
                ir: ir.detail.clone(),
                solver_ready: false,
                solver,
                coverage_ready: true,
                coverage: CapabilityDetail::ready(),
                explain_ready: true,
                explain: CapabilityDetail::ready(),
                testgen_ready: true,
                testgen: CapabilityDetail::ready(),
                machine_ir_error: Some(error),
                reasons: ir.codes,
            }
        }
    }
}

fn machine_solver_capability_assessment(model: &ModelIr) -> CapabilityAssessment {
    let mut codes = BTreeSet::new();
    let mut unsupported_features = BTreeSet::new();
    #[cfg(any(debug_assertions, feature = "verification-runtime"))]
    for field in &model.state_fields {
        if matches!(field.ty, FieldType::String { .. }) {
            codes.insert("string_fields_require_explicit_backend".to_string());
            unsupported_features.insert(format!("state field `{}`: String", field.name));
        }
    }
    for action in &model.actions {
        collect_solver_subset_reasons_from_expr(
            &action.guard,
            &mut codes,
            &mut unsupported_features,
        );
        for update in &action.updates {
            collect_solver_subset_reasons_from_expr(
                &update.value,
                &mut codes,
                &mut unsupported_features,
            );
        }
    }
    for property in &model.properties {
        collect_solver_subset_reasons_from_expr(
            &property.expr,
            &mut codes,
            &mut unsupported_features,
        );
        if matches!(property.kind, crate::ir::PropertyKind::DeadlockFreedom) {
            codes.insert("deadlock_freedom_requires_explicit_backend".to_string());
        }
    }
    let codes = codes.into_iter().collect::<Vec<_>>();
    if codes.is_empty() {
        CapabilityAssessment::ready()
    } else {
        CapabilityAssessment {
            codes,
            detail: CapabilityDetail::blocked(
                "solver backends only support the scalar IR subset; this model still needs the explicit backend",
                "replace String-heavy state and helpers with finite enums or bounded integers, or keep running with `--backend explicit` until solver encodings land",
                unsupported_features.into_iter().collect(),
            ),
        }
    }
}

fn collect_solver_subset_reasons_from_expr(
    expr: &ExprIr,
    reasons: &mut BTreeSet<String>,
    unsupported_features: &mut BTreeSet<String>,
) {
    #[cfg(any(debug_assertions, feature = "verification-runtime"))]
    match expr {
        ExprIr::Literal(Value::String(_)) => {
            reasons.insert("string_literals_require_explicit_backend".to_string());
            unsupported_features.insert("string literal".to_string());
        }
        ExprIr::Unary { op, expr } => {
            if matches!(op, UnaryOp::StringLen) {
                reasons.insert("string_ops_require_explicit_backend".to_string());
                unsupported_features.insert("len(...)".to_string());
            }
            collect_solver_subset_reasons_from_expr(expr, reasons, unsupported_features);
        }
        ExprIr::Binary { op, left, right } => {
            if matches!(op, BinaryOp::StringContains | BinaryOp::RegexMatch) {
                reasons.insert("string_ops_require_explicit_backend".to_string());
            }
            if matches!(op, BinaryOp::StringContains) {
                unsupported_features.insert("str_contains(...)".to_string());
            }
            if matches!(op, BinaryOp::RegexMatch) {
                reasons.insert("regex_match_requires_explicit_backend".to_string());
                unsupported_features.insert("regex_match(...)".to_string());
            }
            collect_solver_subset_reasons_from_expr(left, reasons, unsupported_features);
            collect_solver_subset_reasons_from_expr(right, reasons, unsupported_features);
        }
        ExprIr::Literal(_) | ExprIr::FieldRef(_) => {}
    }
}

fn machine_ir_capability_assessment<M: VerifiedMachine>(error: &str) -> CapabilityAssessment {
    let mut codes = BTreeSet::new();
    let mut unsupported_features = BTreeSet::new();
    let mut reason = None;
    let mut migration_hint = None;

    if M::transitions().is_empty() {
        codes.insert("opaque_step_closure".to_string());
        codes.insert("missing_declarative_transitions".to_string());
        unsupported_features.insert("step(state, action)".to_string());
        reason.get_or_insert_with(|| {
            "opaque step models cannot be lowered into machine IR".to_string()
        });
        migration_hint.get_or_insert_with(|| {
            "rewrite the model using declarative `transitions { ... }` blocks so guards and updates become first-class IR".to_string()
        });
    }
    if error.contains("requires exactly one init state") {
        codes.insert("multiple_init_states".to_string());
        unsupported_features.insert("multiple init states".to_string());
        reason.get_or_insert_with(|| {
            "machine IR currently requires exactly one initial state".to_string()
        });
        migration_hint.get_or_insert_with(|| {
            "collapse the init cases into a single representative initial state, then branch in the first transition if needed".to_string()
        });
    }
    if error.contains("unsupported machine guard expression") {
        codes.insert("unsupported_machine_guard_expr".to_string());
        unsupported_features.extend(
            backtick_segments(error)
                .into_iter()
                .map(|segment| format!("guard: {segment}")),
        );
        reason.get_or_insert_with(|| {
            "one or more declarative guards use syntax outside the current machine IR subset"
                .to_string()
        });
        migration_hint.get_or_insert_with(|| {
            "simplify guard expressions to the current IR subset, or extend lowering support for the reported guard form".to_string()
        });
    }
    if error.contains("unsupported machine update expression") {
        codes.insert("unsupported_machine_update_expr".to_string());
        unsupported_features.extend(
            backtick_segments(error)
                .into_iter()
                .map(|segment| format!("update: {segment}")),
        );
        reason.get_or_insert_with(|| {
            "one or more transition updates use syntax outside the current machine IR subset"
                .to_string()
        });
        migration_hint.get_or_insert_with(|| {
            "rewrite transition updates with the supported arithmetic/boolean subset, or add lowering support for the reported update form".to_string()
        });
    }
    if error.contains("not representable in the current IR subset") {
        codes.insert("unsupported_machine_property_expr".to_string());
        unsupported_features.extend(
            backtick_segments(error)
                .into_iter()
                .map(|segment| format!("property: {segment}")),
        );
        reason.get_or_insert_with(|| {
            "one or more properties cannot be represented in the current machine IR subset"
                .to_string()
        });
        migration_hint.get_or_insert_with(|| {
            "keep property expressions within the supported boolean/arithmetic subset for machine IR and solver-backed checks".to_string()
        });
    }
    if error.contains("unsupported rust field type") {
        codes.insert("unsupported_rust_field_type".to_string());
        unsupported_features.extend(
            backtick_segments(error)
                .into_iter()
                .map(|segment| format!("field type: {segment}")),
        );
        reason.get_or_insert_with(|| {
            "one or more state fields use Rust types that machine IR does not support".to_string()
        });
        migration_hint.get_or_insert_with(|| {
            "replace the field with a supported scalar type, String, or a finite enum/set/relation/map".to_string()
        });
    }
    if error.contains("exceeds supported") {
        codes.insert("unsupported_field_range".to_string());
        unsupported_features.extend(
            backtick_segments(error)
                .into_iter()
                .map(|segment| format!("range/detail: {segment}")),
        );
        reason.get_or_insert_with(|| {
            "one or more declared field ranges exceed the current machine IR bounds".to_string()
        });
        migration_hint.get_or_insert_with(|| {
            "narrow the declared field range to the supported bounds for the chosen scalar type"
                .to_string()
        });
    }

    let codes = codes.into_iter().collect::<Vec<_>>();
    if codes.is_empty() {
        let unsupported_features = backtick_segments(error);
        CapabilityAssessment {
            codes: vec!["machine_ir_lowering_failed".to_string()],
            detail: CapabilityDetail::blocked(
                "machine IR lowering failed for a construct outside the current subset",
                "inspect the reported construct, then simplify it or extend lowering support before retrying solver-backed tooling",
                unsupported_features,
            ),
        }
    } else {
        CapabilityAssessment {
            codes,
            detail: CapabilityDetail::blocked(
                reason.unwrap_or_else(|| {
                    "machine IR lowering failed for a construct outside the current subset"
                        .to_string()
                }),
                migration_hint.unwrap_or_else(|| {
                    "simplify the reported construct so it fits the current machine IR subset"
                        .to_string()
                }),
                unsupported_features.into_iter().collect(),
            ),
        }
    }
}

fn solver_capability_blocked_by_ir(ir_detail: &CapabilityDetail) -> CapabilityDetail {
    CapabilityDetail::blocked(
        format!(
            "solver backends require machine IR first; blocking IR reason: {}",
            if ir_detail.reason.is_empty() {
                "machine IR is unavailable".to_string()
            } else {
                ir_detail.reason.clone()
            }
        ),
        ir_detail
            .migration_hint
            .clone()
            .unwrap_or_else(|| "resolve the machine IR blocker first".to_string()),
        ir_detail.unsupported_features.clone(),
    )
}

fn backtick_segments(input: &str) -> Vec<String> {
    sorted_unique_strings(
        input
            .split('`')
            .skip(1)
            .step_by(2)
            .map(str::to_string)
            .collect::<Vec<_>>(),
    )
}

fn sorted_unique_strings(values: Vec<String>) -> Vec<String> {
    values
        .into_iter()
        .filter(|value| !value.is_empty())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}
#[cfg(any(debug_assertions, feature = "verification-runtime"))]
pub fn machine_transition_tags_for_action<M: ModelSpec>(action_id: &str) -> Vec<String> {
    machine_transition_path_for_action::<M>(action_id, true).legacy_path_tags()
}

pub fn machine_transition_path_for_action<M: ModelSpec>(
    action_id: &str,
    guard_enabled: bool,
) -> Path {
    let mut path = Path::default();
    for transition in machine_transition_ir::<M>()
        .into_iter()
        .filter(|transition| transition.action_id == action_id)
    {
        path.extend(build_path_from_parts(
            transition.action_id,
            &transition
                .reads
                .iter()
                .map(|item| item.to_string())
                .collect::<Vec<_>>(),
            &transition
                .writes
                .iter()
                .map(|item| item.to_string())
                .collect::<Vec<_>>(),
            decision_path_tags(
                &transition.path_tags,
                transition.action_id,
                transition.reads.iter().copied(),
                transition.writes.iter().copied(),
                transition.guard,
                transition.effect,
            ),
            transition.guard.map(str::to_string),
            transition
                .updates
                .iter()
                .map(|update| {
                    (
                        update.field.to_string(),
                        update
                            .expr
                            .map(str::to_string)
                            .unwrap_or_else(|| update.field.to_string()),
                    )
                })
                .collect(),
            guard_enabled,
        ));
    }
    path
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
                            is_relation: $crate::valid_state!(@is_relation $( $($meta)+ )?),
                            relation_left_variants: $crate::valid_state!(@relation_left_variants [$field_ty] $( $($meta)+ )?),
                            relation_right_variants: $crate::valid_state!(@relation_right_variants [$field_ty] $( $($meta)+ )?),
                            is_map: $crate::valid_state!(@is_map $( $($meta)+ )?),
                            map_key_variants: $crate::valid_state!(@map_key_variants [$field_ty] $( $($meta)+ )?),
                            map_value_variants: $crate::valid_state!(@map_value_variants [$field_ty] $( $($meta)+ )?),
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
    (@range relation) => {
        None
    };
    (@range map) => {
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
    (@variants [$field_ty:ty] relation) => {
        None
    };
    (@variants [$field_ty:ty] map) => {
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
    (@is_set relation) => {
        false
    };
    (@is_set map) => {
        false
    };
    (@is_set) => {
        false
    };
    (@is_relation relation) => {
        true
    };
    (@is_relation enum) => {
        false
    };
    (@is_relation set) => {
        false
    };
    (@is_relation map) => {
        false
    };
    (@is_relation range = $range:literal) => {
        false
    };
    (@is_relation) => {
        false
    };
    (@relation_left_variants [$field_ty:ty] relation) => {
        Some(<$field_ty as $crate::modeling::FiniteRelationSpec>::left_variant_labels().to_vec())
    };
    (@relation_left_variants [$field_ty:ty] enum) => {
        None
    };
    (@relation_left_variants [$field_ty:ty] set) => {
        None
    };
    (@relation_left_variants [$field_ty:ty] map) => {
        None
    };
    (@relation_left_variants [$field_ty:ty] range = $range:literal) => {
        None
    };
    (@relation_left_variants [$field_ty:ty]) => {
        None
    };
    (@relation_right_variants [$field_ty:ty] relation) => {
        Some(<$field_ty as $crate::modeling::FiniteRelationSpec>::right_variant_labels().to_vec())
    };
    (@relation_right_variants [$field_ty:ty] enum) => {
        None
    };
    (@relation_right_variants [$field_ty:ty] set) => {
        None
    };
    (@relation_right_variants [$field_ty:ty] map) => {
        None
    };
    (@relation_right_variants [$field_ty:ty] range = $range:literal) => {
        None
    };
    (@relation_right_variants [$field_ty:ty]) => {
        None
    };
    (@is_map map) => {
        true
    };
    (@is_map enum) => {
        false
    };
    (@is_map set) => {
        false
    };
    (@is_map relation) => {
        false
    };
    (@is_map range = $range:literal) => {
        false
    };
    (@is_map) => {
        false
    };
    (@map_key_variants [$field_ty:ty] map) => {
        Some(<$field_ty as $crate::modeling::FiniteMapSpec>::key_variant_labels().to_vec())
    };
    (@map_key_variants [$field_ty:ty] enum) => {
        None
    };
    (@map_key_variants [$field_ty:ty] set) => {
        None
    };
    (@map_key_variants [$field_ty:ty] relation) => {
        None
    };
    (@map_key_variants [$field_ty:ty] range = $range:literal) => {
        None
    };
    (@map_key_variants [$field_ty:ty]) => {
        None
    };
    (@map_value_variants [$field_ty:ty] map) => {
        Some(<$field_ty as $crate::modeling::FiniteMapSpec>::value_variant_labels().to_vec())
    };
    (@map_value_variants [$field_ty:ty] enum) => {
        None
    };
    (@map_value_variants [$field_ty:ty] set) => {
        None
    };
    (@map_value_variants [$field_ty:ty] relation) => {
        None
    };
    (@map_value_variants [$field_ty:ty] range = $range:literal) => {
        None
    };
    (@map_value_variants [$field_ty:ty]) => {
        None
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
                            is_relation: $crate::valid_state!(@is_relation $( $($meta)+ )?),
                            relation_left_variants: $crate::valid_state!(@relation_left_variants [$field_ty] $( $($meta)+ )?),
                            relation_right_variants: $crate::valid_state!(@relation_right_variants [$field_ty] $( $($meta)+ )?),
                            is_map: $crate::valid_state!(@is_map $( $($meta)+ )?),
                            map_key_variants: $crate::valid_state!(@map_key_variants [$field_ty] $( $($meta)+ )?),
                            map_value_variants: $crate::valid_state!(@map_value_variants [$field_ty] $( $($meta)+ )?),
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
macro_rules! valid_model_transition_push {
    ($next_states:ident, [$state_ctor:ident { $($field:ident : $update_expr:expr,)* .. $rest_state:ident $(,)? }]) => {
        $next_states.push($state_ctor {
            $($field: $update_expr,)*
            ..$rest_state.clone()
        });
    };
    ($next_states:ident, [$state_ctor:ident { $($field:ident : $update_expr:expr,)* .. $($unsupported_rest:tt)+ }]) => {
        compile_error!(
            "declarative transition struct updates support only `..state`-style identifiers; write `..state`, not an arbitrary expression"
        );
    };
    ($next_states:ident, [$state_ctor:ident { $($field:ident : $update_expr:expr),* $(,)? }]) => {
        $next_states.push($state_ctor { $($field: $update_expr),* });
    };
    ($next_states:ident, [$($next_state:expr),* $(,)?]) => {
        $next_states.extend(vec![$($next_state),*]);
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! valid_model_path_tags {
    ($($path_tag:literal),*) => {
        &[$($path_tag),*]
    };
    () => {
        &[]
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! valid_model_transition_role {
    (setup) => {
        $crate::ir::action::ActionRole::Setup
    };
    (business) => {
        $crate::ir::action::ActionRole::Business
    };
    () => {
        $crate::ir::action::ActionRole::Business
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! valid_model_transition_descriptor {
    (
        [$action_ty:ty]
        $transition_action:ident
        $( [ role = $role:ident ] )?
        $( [ tags = [$($path_tag:literal),* $(,)?] ] )?
        |$guard_state:ident| $guard_expr:expr
        => [$state_ctor:ident { $($field:ident : $update_expr:expr,)* .. $rest_state:ident $(,)? }]
    ) => {{
        let descriptor = $crate::modeling::action_descriptor_by_variant::<$action_ty>(
            stringify!($transition_action)
        );
        $crate::modeling::TransitionDescriptor {
            action_variant: descriptor.variant,
            action_id: descriptor.action_id,
            role: $crate::valid_model_transition_role!($($role)?),
            guard: stringify!($guard_expr),
            effect: stringify!($state_ctor { $($field: $update_expr,)* ..$rest_state }),
            reads: descriptor.reads,
            writes: descriptor.writes,
            path_tags: $crate::valid_model_path_tags!($($($path_tag),*)?),
            updates: &[
                $(
                    $crate::modeling::TransitionUpdateDescriptor {
                        field: stringify!($field),
                        expr: stringify!($update_expr),
                    }
                ),*
            ],
        }
    }};
    (
        [$action_ty:ty]
        $transition_action:ident
        $( [ role = $role:ident ] )?
        $( [ tags = [$($path_tag:literal),* $(,)?] ] )?
        |$guard_state:ident| $guard_expr:expr
        => [$state_ctor:ident { $($field:ident : $update_expr:expr,)* .. $($unsupported_rest:tt)+ }]
    ) => {{
        compile_error!(
            "declarative transition struct updates support only `..state`-style identifiers; write `..state`, not an arbitrary expression"
        );
    }};
    (
        [$action_ty:ty]
        $transition_action:ident
        $( [ role = $role:ident ] )?
        $( [ tags = [$($path_tag:literal),* $(,)?] ] )?
        |$guard_state:ident| $guard_expr:expr
        => [$state_ctor:ident { $($field:ident : $update_expr:expr),* $(,)? }]
    ) => {{
        let descriptor = $crate::modeling::action_descriptor_by_variant::<$action_ty>(
            stringify!($transition_action)
        );
        $crate::modeling::TransitionDescriptor {
            action_variant: descriptor.variant,
            action_id: descriptor.action_id,
            role: $crate::valid_model_transition_role!($($role)?),
            guard: stringify!($guard_expr),
            effect: stringify!($state_ctor { $($field: $update_expr),* }),
            reads: descriptor.reads,
            writes: descriptor.writes,
            path_tags: $crate::valid_model_path_tags!($($($path_tag),*)?),
            updates: &[
                $(
                    $crate::modeling::TransitionUpdateDescriptor {
                        field: stringify!($field),
                        expr: stringify!($update_expr),
                    }
                ),*
            ],
        }
    }};
    (
        [$action_ty:ty]
        $transition_action:ident
        $( [ role = $role:ident ] )?
        $( [ tags = [$($path_tag:literal),* $(,)?] ] )?
        |$guard_state:ident| $guard_expr:expr
        => [$($next_state:expr),* $(,)?]
    ) => {{
        let descriptor = $crate::modeling::action_descriptor_by_variant::<$action_ty>(
            stringify!($transition_action)
        );
        $crate::modeling::TransitionDescriptor {
            action_variant: descriptor.variant,
            action_id: descriptor.action_id,
            role: $crate::valid_model_transition_role!($($role)?),
            guard: stringify!($guard_expr),
            effect: stringify!([$($next_state),*]),
            reads: descriptor.reads,
            writes: descriptor.writes,
            path_tags: $crate::valid_model_path_tags!($($($path_tag),*)?),
            updates: &[],
        }
    }};
}

#[doc(hidden)]
#[macro_export]
macro_rules! valid_model_push_properties {
    ($properties:ident [$model:ident] [$state_ty:ty]; assume $property:ident |$holds_state:ident| $holds_expr:expr; $($rest:tt)*) => {
        $properties.push($crate::modeling::ModelProperty::assume_invariant_expr(
            stringify!($property),
            Some(stringify!($holds_expr)),
            |$holds_state: &$state_ty| $holds_expr,
        ));
        $crate::valid_model_push_properties!($properties [$model] [$state_ty]; $($rest)*);
    };
    ($properties:ident [$model:ident] [$state_ty:ty]; assert $property:ident |$holds_state:ident| $holds_expr:expr; $($rest:tt)*) => {
        $properties.push($crate::modeling::ModelProperty::assert_invariant_expr(
            stringify!($property),
            Some(stringify!($holds_expr)),
            |$holds_state: &$state_ty| $holds_expr,
        ));
        $crate::valid_model_push_properties!($properties [$model] [$state_ty]; $($rest)*);
    };
    ($properties:ident [$model:ident] [$state_ty:ty]; invariant $property:ident |$holds_state:ident| $holds_expr:expr; $($rest:tt)*) => {
        $properties.push($crate::modeling::ModelProperty::invariant_expr(
            stringify!($property),
            Some(stringify!($holds_expr)),
            |$holds_state: &$state_ty| $holds_expr,
        ));
        $crate::valid_model_push_properties!($properties [$model] [$state_ty]; $($rest)*);
    };
    ($properties:ident [$model:ident] [$state_ty:ty]; deadlock_freedom $property:ident; $($rest:tt)*) => {
        $properties.push($crate::modeling::ModelProperty::deadlock_freedom(
            stringify!($property),
            |state: &$state_ty| !<$model as $crate::modeling::ModelSpec>::enabled_actions(state).is_empty(),
        ));
        $crate::valid_model_push_properties!($properties [$model] [$state_ty]; $($rest)*);
    };
    ($properties:ident [$model:ident] [$state_ty:ty];) => {};
}

#[doc(hidden)]
#[macro_export]
macro_rules! valid_model {
    (
        model $model:ident<$state_ty:ty, $action_ty:ty>;
        init [$($init_state:expr),* $(,)?];
        transitions {
            $(transition $transition_action:ident $( [ role = $role:ident ] )? $( [ tags = [$($path_tag:literal),* $(,)?] ] )? when |$guard_state:ident| $guard_expr:expr => [$($next_state:tt)+];)+
        }
        properties {
            $($property_tokens:tt)+
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
                            $crate::valid_model_transition_push!(next_states, [$($next_state)+]);
                        }
                    }
                )+
                next_states
            }

            fn properties() -> Vec<$crate::modeling::ModelProperty<Self::State>> {
                let mut properties = Vec::new();
                $crate::valid_model_push_properties!(properties [$model] [$state_ty]; $($property_tokens)+);
                properties
            }

            fn transitions() -> Vec<$crate::modeling::TransitionDescriptor> {
                vec![
                    $(
                        $crate::valid_model_transition_descriptor!(
                            [$action_ty]
                            $transition_action
                            $( [ role = $role ] )?
                            $( [ tags = [$($path_tag),*] ] )?
                            |$guard_state| $guard_expr
                            => [$($next_state)+]
                        )
                    ),+
                ]
            }
        }
    };
    (
        model $model:ident<$state_ty:ty, $action_ty:ty>;
        init [$($init_state:expr),* $(,)?];
        step |$state:ident, $action:ident| $step_body:block
        $($rest:tt)*
    ) => {
        compile_error!("step models must use `valid_step_model!`; `valid_model!` only accepts `transitions { ... }`.");
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

#[doc(hidden)]
#[macro_export]
macro_rules! valid_step_model {
    (
        model $model:ident<$state_ty:ty, $action_ty:ty>;
        init [$($init_state:expr),* $(,)?];
        step |$state:ident, $action:ident| $step_body:block
        properties {
            $($property_tokens:tt)+
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
                let mut properties = Vec::new();
                $crate::valid_model_push_properties!(properties [$model] [$state_ty]; $($property_tokens)+);
                properties
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
        $crate::valid_step_model! {
            model $model<$state_ty, $action_ty>;
            init [$($init_state),*];
            step |$state, $action| $step_body
            properties {
                invariant $property |$holds_state| $holds_expr;
            }
        }
    };
    (
        model $model:ident<$state_ty:ty, $action_ty:ty>;
        property $property:ident;
        init [$($init_state:expr),* $(,)?];
        step |$state:ident, $action:ident| $step_body:block
        deadlock_freedom;
    ) => {
        $crate::valid_step_model! {
            model $model<$state_ty, $action_ty>;
            init [$($init_state),*];
            step |$state, $action| $step_body
            properties {
                deadlock_freedom $property;
            }
        }
    };
    (
        model $model:ident;
        $($rest:tt)*
    ) => {
        compile_error!("valid_step_model! requires explicit state/action types. Use `model Name<State, Action>;`.");
    };
    ($($rest:tt)*) => {
        compile_error!("invalid valid_step_model! syntax. Expected `model Name<State, Action>; init [...]; step |state, action| { ... } ...`.");
    };
}

#[cfg(any(debug_assertions, feature = "verification-runtime"))]
#[derive(Debug, Clone)]
struct ModelingNode<S, A> {
    state: S,
    parent: Option<usize>,
    via_action: Option<A>,
    depth: u32,
}

#[cfg(any(debug_assertions, feature = "verification-runtime"))]
#[derive(Debug, Clone)]
struct ModelingEdge<S, A> {
    from_index: usize,
    to_index: usize,
    action: A,
    state_before: S,
    state_after: S,
}

#[cfg(any(debug_assertions, feature = "verification-runtime"))]
pub fn check_machine<M: VerifiedMachine>() -> ModelingCheckResult<M::State, M::Action> {
    let property = primary_property::<M>();
    check_machine_property::<M>(property.property_id)
}

#[cfg(any(debug_assertions, feature = "verification-runtime"))]
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

#[cfg(any(debug_assertions, feature = "verification-runtime"))]
pub fn collect_machine_coverage<M: VerifiedMachine>() -> CoverageReport {
    let exploration = explore_machine::<M>(primary_property::<M>().holds);
    let total_actions = M::Action::all()
        .into_iter()
        .map(|action| action.action_id())
        .collect::<BTreeSet<_>>();
    let total_decisions = total_actions
        .iter()
        .flat_map(|action_id| {
            machine_transition_path_for_action::<M>(action_id, true)
                .decision_ids()
                .into_iter()
                .chain(machine_transition_path_for_action::<M>(action_id, false).decision_ids())
        })
        .collect::<BTreeSet<_>>();
    let mut covered_actions = BTreeSet::new();
    let mut covered_decisions = BTreeSet::new();
    let mut action_execution_counts = BTreeMap::new();
    let mut decision_counts = BTreeMap::new();
    let mut guard_true_actions = BTreeSet::new();
    let mut guard_false_actions = BTreeSet::new();
    let mut guard_true_counts = BTreeMap::new();
    let mut guard_false_counts = BTreeMap::new();
    let mut path_tag_counts = BTreeMap::new();
    let mut covered_requirement_tags = BTreeSet::new();
    let mut requirement_tag_counts = BTreeMap::new();
    let mut depth_histogram = BTreeMap::new();
    let mut repeated_state_count = 0usize;

    for node in &exploration.nodes {
        *depth_histogram.entry(node.depth).or_insert(0) += 1;
        for action in M::Action::all() {
            let next_states = M::step(&node.state, &action);
            if next_states.is_empty() {
                for decision_id in
                    machine_transition_path_for_action::<M>(&action.action_id(), false)
                        .decisions
                        .into_iter()
                        .take(1)
                        .map(|decision| decision.decision_id())
                {
                    covered_decisions.insert(decision_id.clone());
                    *decision_counts.entry(decision_id).or_insert(0) += 1;
                }
                guard_false_actions.insert(action.action_id());
                *guard_false_counts.entry(action.action_id()).or_insert(0) += 1;
            } else {
                for decision_id in
                    machine_transition_path_for_action::<M>(&action.action_id(), true)
                        .decisions
                        .into_iter()
                        .take(1)
                        .map(|decision| decision.decision_id())
                {
                    covered_decisions.insert(decision_id.clone());
                    *decision_counts.entry(decision_id).or_insert(0) += 1;
                }
                guard_true_actions.insert(action.action_id());
                *guard_true_counts.entry(action.action_id()).or_insert(0) += 1;
            }
        }
    }

    for edge in &exploration.edges {
        let action_id = edge.action.action_id();
        covered_actions.insert(action_id.clone());
        *action_execution_counts.entry(action_id).or_insert(0) += 1;
        for decision_id in machine_transition_path_for_action::<M>(&edge.action.action_id(), true)
            .decisions
            .into_iter()
            .skip(1)
            .map(|decision| decision.decision_id())
        {
            covered_decisions.insert(decision_id.clone());
            *decision_counts.entry(decision_id).or_insert(0) += 1;
        }
        for tag in machine_transition_path_for_action::<M>(&edge.action.action_id(), true)
            .legacy_path_tags()
        {
            *path_tag_counts.entry(tag).or_insert(0) += 1;
        }
        for tag in machine_transition_path_for_action::<M>(&edge.action.action_id(), true)
            .legacy_path_tags()
            .into_iter()
            .filter(|tag| {
                !matches!(
                    tag.as_str(),
                    "guard_path" | "read_path" | "write_path" | "transition_path"
                )
            })
        {
            covered_requirement_tags.insert(tag.clone());
            *requirement_tag_counts.entry(tag).or_insert(0) += 1;
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
    let decision_coverage_percent = if total_decisions.is_empty() {
        100
    } else {
        ((covered_decisions.len() * 100) / total_decisions.len()) as u32
    };
    let fully_covered_guards = total_actions
        .iter()
        .filter(|action_id| {
            guard_true_actions.contains(*action_id) && guard_false_actions.contains(*action_id)
        })
        .count();
    let action_roles = machine_transition_ir::<M>()
        .into_iter()
        .map(|transition| {
            (
                transition.action_id.to_string(),
                transition.role.as_str().to_string(),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let business_actions = total_actions
        .iter()
        .filter(|action_id| {
            action_roles
                .get(*action_id)
                .map(|role| role == "business")
                .unwrap_or(true)
        })
        .cloned()
        .collect::<BTreeSet<_>>();
    let setup_actions = total_actions
        .iter()
        .filter(|action_id| {
            action_roles
                .get(*action_id)
                .map(|role| role == "setup")
                .unwrap_or(false)
        })
        .cloned()
        .collect::<BTreeSet<_>>();
    let total_requirement_tags = machine_transition_ir::<M>()
        .into_iter()
        .filter(|transition| transition.role.as_str() == "business")
        .flat_map(|transition| {
            transition.path_tags.into_iter().filter(|tag| {
                !matches!(
                    *tag,
                    "guard_path" | "read_path" | "write_path" | "transition_path"
                )
            })
        })
        .map(str::to_string)
        .collect::<BTreeSet<_>>();
    let guard_full_coverage_percent = if total_actions.is_empty() {
        100
    } else {
        ((fully_covered_guards * 100) / total_actions.len()) as u32
    };
    let business_transition_coverage_percent = if business_actions.is_empty() {
        100
    } else {
        ((covered_actions.intersection(&business_actions).count() * 100) / business_actions.len())
            as u32
    };
    let setup_transition_coverage_percent = if setup_actions.is_empty() {
        100
    } else {
        ((covered_actions.intersection(&setup_actions).count() * 100) / setup_actions.len()) as u32
    };
    let requirement_tag_coverage_percent = if total_requirement_tags.is_empty() {
        100
    } else {
        ((covered_requirement_tags.len() * 100) / total_requirement_tags.len()) as u32
    };
    let business_fully_covered_guards = business_actions
        .iter()
        .filter(|action| {
            guard_true_actions.contains(*action) && guard_false_actions.contains(*action)
        })
        .count();
    let business_guard_full_coverage_percent = if business_actions.is_empty() {
        100
    } else {
        ((business_fully_covered_guards * 100) / business_actions.len()) as u32
    };
    let setup_fully_covered_guards = setup_actions
        .iter()
        .filter(|action| {
            guard_true_actions.contains(*action) && guard_false_actions.contains(*action)
        })
        .count();
    let setup_guard_full_coverage_percent = if setup_actions.is_empty() {
        100
    } else {
        ((setup_fully_covered_guards * 100) / setup_actions.len()) as u32
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
        business_transition_coverage_percent,
        setup_transition_coverage_percent,
        requirement_tag_coverage_percent,
        decision_coverage_percent,
        guard_full_coverage_percent,
        business_guard_full_coverage_percent,
        setup_guard_full_coverage_percent,
        covered_actions,
        covered_decisions,
        total_actions,
        total_decisions,
        action_roles,
        action_execution_counts,
        decision_counts,
        covered_requirement_tags,
        total_requirement_tags,
        requirement_tag_counts,
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

#[cfg(any(debug_assertions, feature = "verification-runtime"))]
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
    let mut decision_path = failure_step.path.clone().unwrap_or_default();
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
    let failing_action_role = transition
        .as_ref()
        .map(|transition| transition.role.as_str().to_string());
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
        decision_path = machine_transition_path_for_action::<M>(transition.action_id, true);
        let path_tags = decision_path.legacy_path_tags();
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
    let property_kind = find_property::<M>(&trace.property_id).property_kind;
    let mut repair_hints = vec![
        "review the action semantics that lead into the violating state".to_string(),
        format!(
            "verify {} property {} is intended",
            property_kind, trace.property_id
        ),
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
    let field_diffs = failure_step
        .state_before
        .iter()
        .filter_map(|(field, before)| {
            let after = failure_step.state_after.get(field)?;
            if before == after {
                None
            } else {
                Some(crate::api::ExplainFieldDiff {
                    field: field.clone(),
                    before: before.clone(),
                    after: after.clone(),
                })
            }
        })
        .collect::<Vec<_>>();
    let guard_reviews = decision_path
        .decisions
        .iter()
        .filter(|decision| matches!(decision.point.kind, crate::ir::DecisionKind::Guard))
        .map(|decision| crate::api::ExplainGuardReview {
            decision_id: decision.decision_id(),
            label: decision.point.label.clone(),
            outcome: match decision.outcome {
                crate::ir::DecisionOutcome::GuardTrue => "guard_true".to_string(),
                crate::ir::DecisionOutcome::GuardFalse => "guard_false".to_string(),
                crate::ir::DecisionOutcome::UpdateApplied => "update_applied".to_string(),
            },
        })
        .collect::<Vec<_>>();
    let repair_targets = vec![
        crate::api::ExplainRepairTargetHint {
            target: "model_fix".to_string(),
            reason: "review the modeled guard/update set around the causal breakpoint".to_string(),
            priority: if !write_overlap_fields.is_empty() {
                "high".to_string()
            } else {
                "medium".to_string()
            },
            action_id: Some(action_id.clone()),
            fields: if write_overlap_fields.is_empty() {
                involved_fields.clone()
            } else {
                write_overlap_fields.clone()
            },
        },
        crate::api::ExplainRepairTargetHint {
            target: "implementation_fix".to_string(),
            reason: format!(
                "inspect the implementation or postcondition of action {} at the failing boundary",
                action_id
            ),
            priority: if involved_fields.is_empty() {
                "medium".to_string()
            } else {
                "high".to_string()
            },
            action_id: Some(action_id.clone()),
            fields: involved_fields.clone(),
        },
    ];

    Ok(ExplainResponse {
        schema_version: "1.0.0".to_string(),
        request_id: request_id.to_string(),
        status: "ok".to_string(),
        evidence_id: trace.evidence_id,
        property_id: trace.property_id,
        property_layer: "assert".to_string(),
        breakpoint_kind: if failure_step
            .note
            .as_deref()
            .is_some_and(|note| note.contains("deadlock"))
        {
            "deadlock_boundary".to_string()
        } else {
            "action_boundary".to_string()
        },
        breakpoint_note: failure_step.note.clone(),
        failure_step_index: failure_step.index,
        failing_action_id: Some(action_id.clone()),
        failing_action_role,
        decision_path,
        failing_action_reads: action_reads,
        failing_action_writes: action_writes,
        failing_action_path_tags: action_path_tags,
        changed_fields: involved_fields.clone(),
        field_diffs,
        guard_reviews,
        write_overlap_fields,
        involved_fields,
        review_context: crate::api::ExplainReviewContext {
            scenario_id: None,
            scenario_expr: None,
            scenario_match_before: None,
            scenario_match_after: None,
            property_scope_expr: None,
            property_scope_match_before: None,
            property_scope_match_after: None,
            vacuous: false,
        },
        candidate_causes,
        repair_targets,
        repair_hints,
        next_steps,
        confidence,
        best_practices: vec![
            "keep actions small so violating transitions stay explainable".to_string(),
            "cover both enabled and disabled outcomes of critical actions".to_string(),
        ],
    })
}

#[cfg(any(debug_assertions, feature = "verification-runtime"))]
pub fn build_machine_test_vectors<M: VerifiedMachine>() -> Vec<TestVector> {
    let property = primary_property::<M>();
    build_machine_test_vectors_for_property::<M>(property.property_id)
}

#[cfg(any(debug_assertions, feature = "verification-runtime"))]
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
    let action_roles = machine_transition_ir::<M>()
        .into_iter()
        .map(|transition| {
            (
                transition.action_id.to_string(),
                transition.role.as_str().to_string(),
            )
        })
        .collect::<BTreeMap<_, _>>();
    for edge in &exploration.edges {
        let first_sequence = vec![edge.action.action_id()];
        if seen_sequences.insert(first_sequence.clone()) {
            let action_id = edge.action.action_id();
            let role = action_roles
                .get(&action_id)
                .map(String::as_str)
                .unwrap_or("business");
            let mut vector = TestVector {
                schema_version: "1.0.0".to_string(),
                vector_id: format!(
                    "vec-{}",
                    stable_hash_hex(&(M::model_id().to_string() + &first_sequence.join(",")))
                        .replace("sha256:", "")
                ),
                run_id: format!(
                    "run-transition-{}",
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
                    action_id: action_id.clone(),
                    action_label: edge.action.action_label(),
                }],
                initial_state: Some(edge.state_before.snapshot()),
                expected_observations: vec![edge.state_after.snapshot()],
                expected_states: vec![format!("{:?}", edge.state_after.snapshot())],
                property_id: property.property_id.to_string(),
                minimized: false,
                focus_action_id: Some(action_id.clone()),
                focus_field: None,
                expected_guard_enabled: Some(true),
                expected_property_holds: Some(true),
                expected_path: machine_transition_path_for_action::<M>(&action_id, true),
                expected_path_tags: machine_transition_tags_for_action::<M>(&action_id),
                setup_action_ids: if role == "setup" {
                    vec![action_id.clone()]
                } else {
                    Vec::new()
                },
                business_action_ids: if role == "business" {
                    vec![action_id.clone()]
                } else {
                    Vec::new()
                },
                notes: machine_transition_tags_for_action::<M>(&action_id)
                    .into_iter()
                    .map(|tag| format!("path_tag:{tag}"))
                    .collect(),
                grouping: crate::testgen::vector_grouping_from_path_tags(
                    &machine_transition_tags_for_action::<M>(&action_id),
                ),
                observation_contract: crate::testgen::ObservationContract::default(),
                observation_layers: Vec::new(),
                oracle_targets: Vec::new(),
                required_inputs: Vec::new(),
                setup_contract: crate::testgen::SetupContract::default(),
                implementation_hints: crate::testgen::ImplementationHints::default(),
                replay_target: None,
            };
            vector.normalize_language_agnostic_contract();
            vectors.push(vector);
        }
    }
    vectors
}

#[cfg(any(debug_assertions, feature = "verification-runtime"))]
pub fn build_machine_test_vectors_for_strategy<M: VerifiedMachine>(
    property_id: Option<&str>,
    strategy: &str,
    focus_action_id: Option<&str>,
) -> Vec<TestVector> {
    let property_id = property_id.unwrap_or_else(|| primary_property::<M>().property_id);
    match strategy {
        "deadlock" => {
            let property = find_property::<M>(property_id);
            if property.property_kind != crate::ir::PropertyKind::DeadlockFreedom {
                Vec::new()
            } else {
                build_machine_test_vectors_for_property::<M>(property_id)
                    .into_iter()
                    .filter(|vector| vector.strategy == "counterexample")
                    .map(|mut vector| {
                        vector.source_kind = "deadlock".to_string();
                        vector.derivation = "deadlock_trace".to_string();
                        vector.strategy = "deadlock".to_string();
                        vector.notes.push("deadlock_reached".to_string());
                        vector
                    })
                    .collect()
            }
        }
        "counterexample" => build_machine_test_vectors_for_property::<M>(property_id)
            .into_iter()
            .filter(|vector| vector.strategy == "counterexample")
            .collect(),
        "transition" | "witness" => build_transition_witness_vectors::<M>(property_id),
        "enablement" => build_enablement_vectors::<M>(property_id, focus_action_id),
        "path" => build_path_tag_vectors::<M>(property_id),
        "guard" => build_guard_coverage_vectors::<M>(property_id),
        "boundary" => build_boundary_focus_vectors::<M>(property_id),
        "random" => build_randomized_vectors::<M>(property_id, 5),
        _ => build_machine_test_vectors_for_property::<M>(property_id),
    }
}

#[cfg(any(debug_assertions, feature = "verification-runtime"))]
fn build_enablement_vectors<M: VerifiedMachine>(
    property_id: &str,
    focus_action_id: Option<&str>,
) -> Vec<TestVector> {
    let Some(target_action_id) = focus_action_id else {
        return Vec::new();
    };
    let property = find_property::<M>(property_id);
    let exploration = explore_machine::<M>(property.holds);
    let transition_ir = machine_transition_ir::<M>();
    let Some(descriptor) = transition_ir
        .iter()
        .find(|transition| transition.action_id == target_action_id)
    else {
        return Vec::new();
    };
    let actions = M::Action::all()
        .into_iter()
        .map(|action| (action.action_id(), action))
        .collect::<BTreeMap<_, _>>();
    let Some(action) = actions.get(target_action_id) else {
        return Vec::new();
    };
    let mut notes = vec![
        format!("enablement_target:{target_action_id}"),
        format!("guard: {}", descriptor.guard.unwrap_or("unknown")),
    ];
    notes.extend(
        machine_transition_tags_for_action::<M>(target_action_id)
            .into_iter()
            .map(|tag| format!("path_tag:{tag}")),
    );

    if let Some((node_index, _)) = exploration
        .nodes
        .iter()
        .enumerate()
        .find(|(_, node)| !M::step(&node.state, action).is_empty())
    {
        let mut reached_notes = notes.clone();
        reached_notes.push("enablement_reached".to_string());
        return build_machine_vector_for_node::<M>(
            &exploration.nodes,
            node_index,
            property.property_id,
            "enablement",
            "enablement",
            Some(target_action_id.to_string()),
            None,
            Some(true),
            reached_notes,
        )
        .into_iter()
        .collect();
    }

    notes.push("enablement_unreachable".to_string());
    build_machine_vector_for_node::<M>(
        &exploration.nodes,
        0,
        property.property_id,
        "enablement",
        "enablement",
        Some(target_action_id.to_string()),
        None,
        Some(false),
        notes,
    )
    .into_iter()
    .collect()
}

#[cfg(any(debug_assertions, feature = "verification-runtime"))]
fn build_transition_witness_vectors<M: VerifiedMachine>(property_id: &str) -> Vec<TestVector> {
    build_machine_test_vectors_for_property::<M>(property_id)
        .into_iter()
        .filter(|vector| vector.source_kind == "witness")
        .collect()
}

#[cfg(any(debug_assertions, feature = "verification-runtime"))]
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

#[cfg(any(debug_assertions, feature = "verification-runtime"))]
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

#[cfg(any(debug_assertions, feature = "verification-runtime"))]
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

#[cfg(any(debug_assertions, feature = "verification-runtime"))]
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

#[cfg(any(debug_assertions, feature = "verification-runtime"))]
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
    let property = find_property::<M>(property_id);
    let property_holds = (property.holds)(&nodes.get(end_index)?.state);
    let expected_path = focus_action_id
        .as_deref()
        .map(|action_id| {
            machine_transition_path_for_action::<M>(
                action_id,
                expected_guard_enabled.unwrap_or(true),
            )
        })
        .unwrap_or_default();
    let expected_path_tags = if expected_path.decisions.is_empty() {
        Vec::new()
    } else {
        expected_path.legacy_path_tags()
    };
    let action_roles = machine_transition_ir::<M>()
        .into_iter()
        .map(|transition| {
            (
                transition.action_id.to_string(),
                transition.role.as_str().to_string(),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let setup_action_ids = actions
        .iter()
        .filter(|step| {
            action_roles
                .get(&step.action_id)
                .map(|role| role == "setup")
                .unwrap_or(false)
        })
        .map(|step| step.action_id.clone())
        .collect::<Vec<_>>();
    let business_action_ids = actions
        .iter()
        .filter(|step| {
            action_roles
                .get(&step.action_id)
                .map(|role| role == "business")
                .unwrap_or(true)
        })
        .map(|step| step.action_id.clone())
        .collect::<Vec<_>>();
    let grouping = crate::testgen::vector_grouping_from_path_tags(&expected_path_tags);
    let mut vector = TestVector {
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
        run_id: format!(
            "run-vector-{}",
            stable_hash_hex(&(M::model_id().to_string() + property_id + &signature))
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
        expected_observations: if trace.is_empty() {
            vec![nodes.get(end_index)?.state.snapshot()]
        } else {
            trace
                .iter()
                .map(|step| step.state_after.snapshot())
                .collect()
        },
        expected_states,
        property_id: property_id.to_string(),
        minimized: false,
        focus_action_id,
        focus_field,
        expected_guard_enabled,
        expected_property_holds: Some(property_holds),
        expected_path,
        expected_path_tags,
        setup_action_ids,
        business_action_ids,
        notes,
        grouping,
        observation_contract: crate::testgen::ObservationContract::default(),
        observation_layers: Vec::new(),
        oracle_targets: Vec::new(),
        required_inputs: Vec::new(),
        setup_contract: crate::testgen::SetupContract::default(),
        implementation_hints: crate::testgen::ImplementationHints::default(),
        replay_target: None,
    };
    vector.normalize_language_agnostic_contract();
    Some(vector)
}

#[cfg(any(debug_assertions, feature = "verification-runtime"))]
fn parse_inclusive_range(range: Option<&'static str>) -> Option<(u64, u64)> {
    let range = range?;
    let (min, max) = range.split_once("..=")?;
    let min = min.parse::<u64>().ok()?;
    let max = max.parse::<u64>().ok()?;
    Some((min, max))
}

#[cfg(any(debug_assertions, feature = "verification-runtime"))]
pub fn lower_machine_model<M: VerifiedMachine>() -> Result<ModelIr, String> {
    run_debug_machine_validation::<M>();
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
                role: transition.role,
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
            let expr = match property.property_kind {
                crate::ir::PropertyKind::Invariant
                | crate::ir::PropertyKind::Reachability
                | crate::ir::PropertyKind::Cover => property
                    .expr
                    .and_then(|expr| lower_machine_expr_with_enums(expr, &enum_literals))
                    .ok_or_else(|| {
                        format!(
                            "machine property `{}` is not representable in the current IR subset",
                            property.property_id
                        )
                    })?,
                crate::ir::PropertyKind::DeadlockFreedom => ExprIr::Literal(Value::Bool(true)),
                crate::ir::PropertyKind::Temporal => {
                    return Err(format!(
                        "machine property `{}` uses temporal operators that are not yet representable in the Rust-first modeling frontend",
                        property.property_id
                    ))
                }
                crate::ir::PropertyKind::Transition => {
                    return Err(format!(
                        "machine property `{}` uses transition postconditions that are not yet representable in the Rust-first modeling frontend",
                        property.property_id
                    ))
                }
            };
            Ok(PropertyIr {
                property_id: property.property_id.to_string(),
                kind: property.property_kind,
                layer: property.property_layer,
                expr,
                scope: None,
                action_filter: None,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;

    Ok(ModelIr {
        model_id: M::model_id().to_string(),
        state_fields,
        init,
        actions,
        predicates: Vec::new(),
        scenarios: Vec::new(),
        properties,
    })
}

#[cfg(any(debug_assertions, feature = "verification-runtime"))]
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
    if field.is_relation {
        let left_variants = field
            .relation_left_variants
            .as_ref()
            .ok_or_else(|| format!("missing left relation variants for field `{}`", field.name))?;
        let right_variants = field
            .relation_right_variants
            .as_ref()
            .ok_or_else(|| format!("missing right relation variants for field `{}`", field.name))?;
        if left_variants.len().saturating_mul(right_variants.len()) > 64 {
            return Err(format!(
                "finite relations currently support at most 64 entries for field `{}`",
                field.name
            ));
        }
        return Ok(FieldType::EnumRelation {
            left_variants: left_variants.iter().map(|item| item.to_string()).collect(),
            right_variants: right_variants.iter().map(|item| item.to_string()).collect(),
        });
    }
    if field.is_map {
        let key_variants = field
            .map_key_variants
            .as_ref()
            .ok_or_else(|| format!("missing map key variants for field `{}`", field.name))?;
        let value_variants = field
            .map_value_variants
            .as_ref()
            .ok_or_else(|| format!("missing map value variants for field `{}`", field.name))?;
        if key_variants.len().saturating_mul(value_variants.len()) > 64 {
            return Err(format!(
                "finite maps currently support at most 64 key/value slots for field `{}`",
                field.name
            ));
        }
        return Ok(FieldType::EnumMap {
            key_variants: key_variants.iter().map(|item| item.to_string()).collect(),
            value_variants: value_variants.iter().map(|item| item.to_string()).collect(),
        });
    }
    match field.rust_type {
        "bool" => Ok(FieldType::Bool),
        "String" => {
            let (min_len, max_len) = match parse_inclusive_range(field.range) {
                Some((min, max)) => {
                    if max > u32::MAX as u64 {
                        return Err(format!(
                            "range `{}` exceeds supported string length bounds for field `{}`",
                            field.range.unwrap_or("0..=4294967295"),
                            field.name
                        ));
                    }
                    (Some(min as u32), Some(max as u32))
                }
                None => (None, None),
            };
            Ok(FieldType::String { min_len, max_len })
        }
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

#[cfg(any(debug_assertions, feature = "verification-runtime"))]
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
        if field.is_relation {
            if let (Some(left_variants), Some(right_variants)) = (
                field.relation_left_variants.as_ref(),
                field.relation_right_variants.as_ref(),
            ) {
                if let Some((left_ty, right_ty)) = relation_inner_rust_types(field.rust_type) {
                    register_enum_literals(&mut literals, &left_ty, left_variants);
                    register_enum_literals(&mut literals, &right_ty, right_variants);
                }
            }
        }
        if field.is_map {
            if let (Some(key_variants), Some(value_variants)) = (
                field.map_key_variants.as_ref(),
                field.map_value_variants.as_ref(),
            ) {
                if let Some((key_ty, value_ty)) = map_inner_rust_types(field.rust_type) {
                    register_enum_literals(&mut literals, &key_ty, key_variants);
                    register_enum_literals(&mut literals, &value_ty, value_variants);
                }
            }
        }
    }
    literals
}

#[cfg(any(debug_assertions, feature = "verification-runtime"))]
fn register_enum_literals(
    literals: &mut BTreeMap<String, (String, u64)>,
    enum_ty: &str,
    variants: &[&str],
) {
    for (index, variant) in variants.iter().enumerate() {
        literals.insert(
            format!("{enum_ty}::{variant}"),
            ((*variant).to_string(), index as u64),
        );
        literals
            .entry((*variant).to_string())
            .or_insert_with(|| ((*variant).to_string(), index as u64));
        if let Some(inner_ty) = option_inner_rust_type(enum_ty) {
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

#[cfg(any(debug_assertions, feature = "verification-runtime"))]
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

#[cfg(any(debug_assertions, feature = "verification-runtime"))]
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

#[cfg(any(debug_assertions, feature = "verification-runtime"))]
fn relation_inner_rust_types(rust_type: &str) -> Option<(String, String)> {
    let normalized = rust_type
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .collect::<String>();
    let inner = normalized
        .strip_prefix("FiniteRelation<")
        .and_then(|value| value.strip_suffix('>'))?;
    let parts = inner.split(',').map(str::to_string).collect::<Vec<_>>();
    if parts.len() == 2 && parts.iter().all(|value| !value.is_empty()) {
        Some((parts[0].clone(), parts[1].clone()))
    } else {
        None
    }
}

#[cfg(any(debug_assertions, feature = "verification-runtime"))]
fn map_inner_rust_types(rust_type: &str) -> Option<(String, String)> {
    let normalized = rust_type
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .collect::<String>();
    let inner = normalized
        .strip_prefix("FiniteMap<")
        .and_then(|value| value.strip_suffix('>'))?;
    let parts = inner.split(',').map(str::to_string).collect::<Vec<_>>();
    if parts.len() == 2 && parts.iter().all(|value| !value.is_empty()) {
        Some((parts[0].clone(), parts[1].clone()))
    } else {
        None
    }
}

fn lower_machine_expr_with_enums(
    input: &str,
    enum_literals: &BTreeMap<String, (String, u64)>,
) -> Option<ExprIr> {
    let trimmed = strip_wrapping_machine_parens(input.trim());
    let normalized = trimmed.split_whitespace().collect::<Vec<_>>().join(" ");
    let normalized = normalized.trim();
    let normalized = normalized.strip_prefix('&').unwrap_or(normalized).trim();
    let normalized = normalized
        .strip_prefix("state.")
        .unwrap_or(normalized)
        .trim();
    if normalized == "true" {
        return Some(ExprIr::Literal(Value::Bool(true)));
    }
    if normalized == "false" {
        return Some(ExprIr::Literal(Value::Bool(false)));
    }
    if let Some(inner) = normalized.strip_suffix(".to_string()") {
        return lower_machine_expr_with_enums(inner.trim(), enum_literals);
    }
    if let Some([inner]) = function_args_machine(normalized, "String::from") {
        return lower_machine_expr_with_enums(inner, enum_literals);
    }
    if let Some(value) = parse_rust_string_literal(normalized) {
        return Some(ExprIr::Literal(Value::String(value)));
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
    if let Some([value]) = function_args_machine(normalized, "len") {
        return Some(ExprIr::Unary {
            op: UnaryOp::StringLen,
            expr: Box::new(lower_machine_expr_with_enums(value, enum_literals)?),
        });
    }
    if let Some([value, needle]) = function_args_machine(normalized, "str_contains") {
        return Some(ExprIr::Binary {
            op: BinaryOp::StringContains,
            left: Box::new(lower_machine_expr_with_enums(value, enum_literals)?),
            right: Box::new(lower_machine_expr_with_enums(needle, enum_literals)?),
        });
    }
    if let Some([value, pattern]) = function_args_machine(normalized, "regex_match") {
        return Some(ExprIr::Binary {
            op: BinaryOp::RegexMatch,
            left: Box::new(lower_machine_expr_with_enums(value, enum_literals)?),
            right: Box::new(lower_machine_expr_with_enums(pattern, enum_literals)?),
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
    if let Some([relation, left, right]) = function_args_machine(normalized, "rel_contains") {
        return Some(ExprIr::Binary {
            op: BinaryOp::RelationContains,
            left: Box::new(lower_machine_expr_with_enums(relation, enum_literals)?),
            right: Box::new(pair_literal_expr(left, right, enum_literals)?),
        });
    }
    if let Some([relation, left, right]) = function_args_machine(normalized, "rel_insert") {
        return Some(ExprIr::Binary {
            op: BinaryOp::RelationInsert,
            left: Box::new(lower_machine_expr_with_enums(relation, enum_literals)?),
            right: Box::new(pair_literal_expr(left, right, enum_literals)?),
        });
    }
    if let Some([relation, left, right]) = function_args_machine(normalized, "rel_remove") {
        return Some(ExprIr::Binary {
            op: BinaryOp::RelationRemove,
            left: Box::new(lower_machine_expr_with_enums(relation, enum_literals)?),
            right: Box::new(pair_literal_expr(left, right, enum_literals)?),
        });
    }
    if let Some([left, right]) = function_args_machine(normalized, "rel_intersects") {
        return Some(ExprIr::Binary {
            op: BinaryOp::RelationIntersects,
            left: Box::new(lower_machine_expr_with_enums(left, enum_literals)?),
            right: Box::new(lower_machine_expr_with_enums(right, enum_literals)?),
        });
    }
    if let Some([map, key]) = function_args_machine(normalized, "map_contains_key") {
        return Some(ExprIr::Binary {
            op: BinaryOp::MapContainsKey,
            left: Box::new(lower_machine_expr_with_enums(map, enum_literals)?),
            right: Box::new(lower_machine_expr_with_enums(key, enum_literals)?),
        });
    }
    if let Some([map, key, value]) = function_args_machine(normalized, "map_contains_entry") {
        return Some(ExprIr::Binary {
            op: BinaryOp::MapContainsEntry,
            left: Box::new(lower_machine_expr_with_enums(map, enum_literals)?),
            right: Box::new(pair_literal_expr(key, value, enum_literals)?),
        });
    }
    if let Some([map, key, value]) = function_args_machine(normalized, "map_put") {
        return Some(ExprIr::Binary {
            op: BinaryOp::MapPut,
            left: Box::new(lower_machine_expr_with_enums(map, enum_literals)?),
            right: Box::new(pair_literal_expr(key, value, enum_literals)?),
        });
    }
    if let Some([map, key]) = function_args_machine(normalized, "map_remove") {
        return Some(ExprIr::Binary {
            op: BinaryOp::MapRemoveKey,
            left: Box::new(lower_machine_expr_with_enums(map, enum_literals)?),
            right: Box::new(lower_machine_expr_with_enums(key, enum_literals)?),
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

fn parse_rust_string_literal(input: &str) -> Option<String> {
    let trimmed = input.trim();
    if let Some(inner) = trimmed
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
    {
        return Some(unescape_basic_string(inner));
    }
    if let Some(rest) = trimmed.strip_prefix('r') {
        let hashes = rest.chars().take_while(|ch| *ch == '#').count();
        let prefix_len = 1 + hashes;
        let quote_prefix = format!("r{}\"", "#".repeat(hashes));
        let quote_suffix = format!("\"{}", "#".repeat(hashes));
        if trimmed.starts_with(&quote_prefix) && trimmed.ends_with(&quote_suffix) {
            let inner = &trimmed[prefix_len + 1..trimmed.len() - quote_suffix.len()];
            return Some(inner.to_string());
        }
    }
    None
}

fn unescape_basic_string(input: &str) -> String {
    let mut out = String::new();
    let mut chars = input.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('t') => out.push('\t'),
            Some('\\') => out.push('\\'),
            Some('"') => out.push('"'),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

fn pair_literal_expr(
    left: &str,
    right: &str,
    enum_literals: &BTreeMap<String, (String, u64)>,
) -> Option<ExprIr> {
    let left_expr = lower_machine_expr_with_enums(left, enum_literals)?;
    let right_expr = lower_machine_expr_with_enums(right, enum_literals)?;
    match (left_expr, right_expr) {
        (
            ExprIr::Literal(Value::EnumVariant {
                label: left_label,
                index: left_index,
            }),
            ExprIr::Literal(Value::EnumVariant {
                label: right_label,
                index: right_index,
            }),
        ) => Some(ExprIr::Literal(Value::PairVariant {
            left_label,
            left_index,
            right_label,
            right_index,
        })),
        _ => None,
    }
}

fn function_args_machine<'a, const N: usize>(input: &'a str, name: &str) -> Option<[&'a str; N]> {
    let rest = input.strip_prefix(name)?.trim_start();
    let rest = rest.strip_prefix('(')?;
    let mut depth = 1usize;
    let mut end_index = None;
    for (index, ch) in rest.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    end_index = Some(index);
                    break;
                }
            }
            _ => {}
        }
    }
    let end_index = end_index?;
    if !rest[end_index + 1..].trim().is_empty() {
        return None;
    }
    let call = &rest[..end_index];
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

#[cfg(any(debug_assertions, feature = "verification-runtime"))]
pub fn check_machine_outcome<M: VerifiedMachine>(request_id: &str) -> CheckOutcome {
    check_machine_outcome_with_seed::<M>(request_id, None)
}

pub fn check_machine_outcome_with_seed<M: VerifiedMachine>(
    request_id: &str,
    seed: Option<u64>,
) -> CheckOutcome {
    let property = primary_property::<M>();
    check_machine_outcome_for_property_with_seed::<M>(request_id, property.property_id, seed)
}

#[cfg(any(debug_assertions, feature = "verification-runtime"))]
pub fn check_machine_outcome_for_property<M: VerifiedMachine>(
    request_id: &str,
    property_id: &str,
) -> CheckOutcome {
    check_machine_outcome_for_property_with_seed::<M>(request_id, property_id, None)
}

pub fn check_machine_outcome_for_property_with_seed<M: VerifiedMachine>(
    request_id: &str,
    property_id: &str,
    seed: Option<u64>,
) -> CheckOutcome {
    let result = check_machine_property::<M>(property_id);
    let property = find_property::<M>(result.property_id);
    let property_kind = property.property_kind.clone();
    let run_id = format!(
        "run-{}",
        stable_hash_hex(&(request_id.to_string() + M::model_id() + result.property_id))
            .replace("sha256:", "")
    );
    let source_hash = stable_hash_hex(M::model_id());
    let contract_hash = stable_hash_hex(&(M::model_id().to_string() + result.property_id));
    let manifest = build_run_manifest(
        request_id.to_string(),
        run_id.clone(),
        source_hash,
        contract_hash,
        BackendKind::Explicit,
        env!("CARGO_PKG_VERSION").to_string(),
        seed,
    );

    let (status, reason_code, summary, trace) = match result.status {
        ModelingRunStatus::Pass => (
            RunStatus::Pass,
            Some("COMPLETE_SPACE_EXHAUSTED".to_string()),
            match &property_kind {
                crate::ir::PropertyKind::Invariant => {
                    "no violating state found in the reachable state space".to_string()
                }
                crate::ir::PropertyKind::Reachability => {
                    "reachability target was not found in the reachable state space".to_string()
                }
                crate::ir::PropertyKind::Cover => {
                    "cover target was not reached in the reachable state space".to_string()
                }
                crate::ir::PropertyKind::Transition => {
                    "no violating transition was found in the reachable state space".to_string()
                }
                crate::ir::PropertyKind::DeadlockFreedom => {
                    "no deadlock state found in the reachable state space".to_string()
                }
                crate::ir::PropertyKind::Temporal => {
                    "temporal property is not supported in the Rust-first modeling frontend"
                        .to_string()
                }
            },
            None,
        ),
        ModelingRunStatus::Fail => {
            let trace = build_evidence_trace::<M>(request_id, &result);
            (
                RunStatus::Fail,
                Some(match &property_kind {
                    crate::ir::PropertyKind::Invariant => "PROPERTY_VIOLATED".to_string(),
                    crate::ir::PropertyKind::Reachability => "TARGET_REACHED".to_string(),
                    crate::ir::PropertyKind::Cover => "COVER_UNREACHED".to_string(),
                    crate::ir::PropertyKind::Transition => "TRANSITION_PROPERTY_FAILED".to_string(),
                    crate::ir::PropertyKind::DeadlockFreedom => "DEADLOCK_REACHED".to_string(),
                    crate::ir::PropertyKind::Temporal => {
                        "TEMPORAL_PROPERTY_UNSUPPORTED".to_string()
                    }
                }),
                match &property_kind {
                    crate::ir::PropertyKind::Invariant => {
                        "violating state discovered in reachable state space".to_string()
                    }
                    crate::ir::PropertyKind::Reachability => {
                        "reachability target reached in reachable state space".to_string()
                    }
                    crate::ir::PropertyKind::Cover => {
                        "cover target was not reached in reachable state space".to_string()
                    }
                    crate::ir::PropertyKind::Transition => {
                        "transition property failed in reachable state space".to_string()
                    }
                    crate::ir::PropertyKind::DeadlockFreedom => {
                        "deadlock detected in reachable state space".to_string()
                    }
                    crate::ir::PropertyKind::Temporal => {
                        "temporal property is not supported in the Rust-first modeling frontend"
                            .to_string()
                    }
                },
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
            property_kind,
            property_layer: crate::ir::PropertyLayer::Assert,
            status,
            assurance_level: AssuranceLevel::Complete,
            scenario_id: None,
            vacuous: false,
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

#[cfg(any(debug_assertions, feature = "verification-runtime"))]
pub fn check_machine_outcomes<M: VerifiedMachine>(request_id: &str) -> Vec<ExplicitRunResult> {
    check_machine_outcomes_with_seed::<M>(request_id, None)
}

pub fn check_machine_outcomes_with_seed<M: VerifiedMachine>(
    request_id: &str,
    seed: Option<u64>,
) -> Vec<ExplicitRunResult> {
    property_ids::<M>()
        .into_iter()
        .filter_map(|property_id| {
            match check_machine_outcome_for_property_with_seed::<M>(request_id, property_id, seed) {
                CheckOutcome::Completed(result) => Some(result),
                CheckOutcome::Errored(_) => None,
            }
        })
        .collect()
}

#[cfg(any(debug_assertions, feature = "verification-runtime"))]
pub fn check_machine_with_adapter<M: VerifiedMachine>(
    request_id: &str,
    property_id: Option<&str>,
    adapter: &AdapterConfig,
) -> Result<CheckOutcome, String> {
    check_machine_with_adapter_and_seed::<M>(request_id, property_id, adapter, None)
}

pub fn check_machine_with_adapter_and_seed<M: VerifiedMachine>(
    request_id: &str,
    property_id: Option<&str>,
    adapter: &AdapterConfig,
    seed: Option<u64>,
) -> Result<CheckOutcome, String> {
    if matches!(adapter, AdapterConfig::Explicit) {
        return Ok(match property_id {
            Some(property_id) => {
                check_machine_outcome_for_property_with_seed::<M>(request_id, property_id, seed)
            }
            None => {
                let property = primary_property::<M>();
                check_machine_outcome_for_property_with_seed::<M>(
                    request_id,
                    property.property_id,
                    seed,
                )
            }
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
    plan.manifest = build_run_manifest(
        request_id.to_string(),
        format!(
            "run-{}",
            stable_hash_hex(&(request_id.to_string() + &property_id)).replace("sha256:", "")
        ),
        stable_hash_hex(M::model_id()),
        snapshot.contract_hash,
        backend_kind_for_adapter(adapter),
        backend_version_for_adapter(adapter),
        seed,
    );
    plan.property_selection = PropertySelection::ExactlyOne(property_id);
    run_with_adapter(&model, &plan, adapter).map(|normalized| normalized.outcome)
}

#[cfg(any(debug_assertions, feature = "verification-runtime"))]
fn backend_kind_for_adapter(adapter: &AdapterConfig) -> BackendKind {
    match adapter {
        AdapterConfig::Explicit => BackendKind::Explicit,
        AdapterConfig::MockBmc | AdapterConfig::Command { .. } => BackendKind::MockBmc,
        AdapterConfig::SmtCvc5 { .. } => BackendKind::SmtCvc5,
        AdapterConfig::SatVarisat => BackendKind::SatVarisat,
    }
}

#[cfg(any(debug_assertions, feature = "verification-runtime"))]
fn backend_version_for_adapter(adapter: &AdapterConfig) -> String {
    solver_backend_version_for_config(adapter)
}

#[cfg(any(debug_assertions, feature = "verification-runtime"))]
pub fn replay_machine_actions<M: VerifiedMachine>(
    property_id: Option<&str>,
    action_ids: &[String],
    focus_action_id: Option<&str>,
) -> Result<
    (
        BTreeMap<String, Value>,
        &'static str,
        Option<bool>,
        bool,
        Path,
    ),
    String,
> {
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
    let property_holds = (property.holds)(&state);
    let mut path = Path::default();
    for action_id in action_ids {
        path.extend(machine_transition_path_for_action::<M>(action_id, true));
    }
    if let Some(action_id) = focus_action_id {
        let focus_is_already_last = action_ids.last().map(String::as_str) == Some(action_id);
        if !focus_is_already_last {
            let guard_enabled = focus_action_enabled.unwrap_or(false);
            path.extend(Path::new(
                machine_transition_path_for_action::<M>(action_id, guard_enabled)
                    .decisions
                    .into_iter()
                    .take(1)
                    .collect(),
            ));
        }
    }
    Ok((
        state.snapshot(),
        property.property_id,
        focus_action_enabled,
        property_holds,
        path,
    ))
}

#[cfg(any(debug_assertions, feature = "verification-runtime"))]
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

#[cfg(any(debug_assertions, feature = "verification-runtime"))]
#[derive(Debug, Clone)]
struct ModelingExploration<S, A> {
    nodes: Vec<ModelingNode<S, A>>,
    edges: Vec<ModelingEdge<S, A>>,
    explored_transitions: usize,
    visited_states: usize,
    failure_index: Option<usize>,
}

#[cfg(any(debug_assertions, feature = "verification-runtime"))]
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

#[cfg(any(debug_assertions, feature = "verification-runtime"))]
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

#[cfg(any(debug_assertions, feature = "verification-runtime"))]
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
            path: Some(machine_transition_path_for_action::<M>(
                &step.action.action_id(),
                true,
            )),
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
        insert, is_empty, len, lower_machine_expr, lower_machine_expr_with_enums,
        lower_machine_model, machine_capability_report, machine_transition_ir, property_ids,
        regex_match, remove, state_field_descriptors, transition_descriptors, xor, FiniteEnumSet,
        ModelingRunStatus, ModelingState, StateSpec,
    };
    use crate::{
        engine::{CheckOutcome, PropertySelection, RunPlan, RunStatus},
        ir::{BinaryOp, ExprIr, FieldType, Value},
        solver::{run_with_adapter, AdapterConfig},
        valid_actions, valid_state,
    };
    use std::collections::BTreeMap;

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

    crate::valid_step_model! {
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

    crate::valid_step_model! {
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

    valid_state! {
        struct SpreadState {
            note: String [range = "0..=32"],
            approved: bool,
            archived: bool,
        }
    }

    valid_actions! {
        enum SpreadAction {
            Approve => "APPROVE" [reads = ["approved"], writes = ["approved"]],
            Archive => "ARCHIVE" [reads = ["approved", "archived"], writes = ["note", "approved", "archived"]],
        }
    }

    crate::valid_model! {
        model SpreadModel<SpreadState, SpreadAction>;
        init [SpreadState {
            note: "draft".to_string(),
            approved: false,
            archived: false,
        }];
        transitions {
            transition Approve [tags = ["approval_path"]] when |state| state.approved == false => [SpreadState {
                approved: true,
                ..state
            }];
            transition Archive [tags = ["archive_path"]] when |state| state.approved && state.archived == false => [SpreadState {
                note: "archived".to_string(),
                approved: state.approved,
                archived: true,
            }];
        }
        properties {
            invariant P_ARCHIVE_REQUIRES_APPROVAL |state| state.archived == false || state.approved;
        }
    }

    #[test]
    fn declarative_transition_struct_updates_clone_unchanged_fields() {
        let init = <SpreadModel as crate::modeling::ModelSpec>::init_states()
            .into_iter()
            .next()
            .expect("spread model init state");
        let next_states =
            <SpreadModel as crate::modeling::ModelSpec>::step(&init, &SpreadAction::Approve);
        assert_eq!(
            next_states,
            vec![SpreadState {
                note: "draft".to_string(),
                approved: true,
                archived: false,
            }]
        );
    }

    #[test]
    fn declarative_transition_struct_updates_preserve_explicit_update_metadata() {
        let transitions = machine_transition_ir::<SpreadModel>();
        assert_eq!(transitions.len(), 2);
        assert_eq!(transitions[0].action_id, "APPROVE");
        assert!(transitions[0]
            .effect
            .expect("spread transition effect")
            .contains(".. state"));
        assert_eq!(transitions[0].updates.len(), 1);
        assert_eq!(transitions[0].updates[0].field, "approved");
        assert_eq!(transitions[0].updates[0].expr, Some("true"));
        assert_eq!(transitions[1].updates.len(), 3);
    }

    #[test]
    fn declarative_transition_struct_updates_lower_with_mixed_literal_forms() {
        let model =
            lower_machine_model::<SpreadModel>().expect("spread model lowering should work");
        assert_eq!(model.actions.len(), 2);
        assert_eq!(model.actions[0].action_id, "APPROVE");
        assert_eq!(model.actions[0].updates.len(), 1);
        assert_eq!(model.actions[0].updates[0].field, "approved");
        assert!(matches!(model.state_fields[0].ty, FieldType::String { .. }));
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

    #[test]
    fn lower_machine_expr_supports_relation_and_map_ops() {
        let enum_literals = BTreeMap::from([
            ("Member::Alice".to_string(), ("Alice".to_string(), 0)),
            ("Tenant::Alpha".to_string(), ("Alpha".to_string(), 0)),
            (
                "Plan::Enterprise".to_string(),
                ("Enterprise".to_string(), 1),
            ),
        ]);
        let split = super::split_top_level_machine(
            "rel_contains(state.memberships, Member::Alice, Tenant::Alpha) && map_contains_entry(state.plans, Tenant::Alpha, Plan::Enterprise)",
            "&&",
        )
        .expect("combined expression should split");
        assert_eq!(
            split.0.trim(),
            "rel_contains(state.memberships, Member::Alice, Tenant::Alpha)"
        );
        assert_eq!(
            split.1.trim(),
            "map_contains_entry(state.plans, Tenant::Alpha, Plan::Enterprise)"
        );
        let relation = lower_machine_expr_with_enums(
            "rel_contains(state.memberships, Member::Alice, Tenant::Alpha)",
            &enum_literals,
        )
        .expect("relation expression should lower");
        let map = lower_machine_expr_with_enums(
            "map_contains_entry(state.plans, Tenant::Alpha, Plan::Enterprise)",
            &enum_literals,
        )
        .expect("map expression should lower");
        let combined = lower_machine_expr_with_enums(
            "rel_contains(state.memberships, Member::Alice, Tenant::Alpha) && map_contains_entry(state.plans, Tenant::Alpha, Plan::Enterprise)",
            &enum_literals,
        )
        .expect("combined expression should lower");
        let relation_debug = format!("{relation:?}");
        let map_debug = format!("{map:?}");
        let combined_debug = format!("{combined:?}");
        assert!(relation_debug.contains("RelationContains"));
        assert!(map_debug.contains("MapContainsEntry"));
        assert!(combined_debug.contains("And"));
    }

    #[test]
    fn relation_and_map_state_fields_expose_metadata() {
        #[allow(dead_code)]
        #[derive(Debug, Clone, PartialEq, Eq, Hash, crate::ValidEnum)]
        enum Member {
            Alice,
            Bob,
        }

        #[allow(dead_code)]
        #[derive(Debug, Clone, PartialEq, Eq, Hash, crate::ValidEnum)]
        enum Tenant {
            Alpha,
            Beta,
        }

        #[allow(dead_code)]
        #[derive(Debug, Clone, PartialEq, Eq, Hash, crate::ValidEnum)]
        enum Plan {
            Free,
            Enterprise,
        }

        valid_state! {
            struct RelationMapState {
                memberships: crate::modeling::FiniteRelation<Member, Tenant> [relation],
                plans: crate::modeling::FiniteMap<Tenant, Plan> [map],
            }
        }

        let fields = RelationMapState::state_fields();
        assert_eq!(fields.len(), 2);
        assert!(fields[0].is_relation);
        assert_eq!(
            fields[0].relation_left_variants.as_deref(),
            Some(vec!["Alice", "Bob"].as_slice())
        );
        assert_eq!(
            fields[0].relation_right_variants.as_deref(),
            Some(vec!["Alpha", "Beta"].as_slice())
        );
        assert!(fields[1].is_map);
        assert_eq!(
            fields[1].map_key_variants.as_deref(),
            Some(vec!["Alpha", "Beta"].as_slice())
        );
        assert_eq!(
            fields[1].map_value_variants.as_deref(),
            Some(vec!["Free", "Enterprise"].as_slice())
        );
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
        assert!(report.ir.reason.contains("opaque step models"));
        assert_eq!(
            report.ir.migration_hint.as_deref(),
            Some(
                "rewrite the model using declarative `transitions { ... }` blocks so guards and updates become first-class IR"
            )
        );
        assert!(report
            .ir
            .unsupported_features
            .contains(&"step(state, action)".to_string()));
        assert!(report.reasons.contains(&"opaque_step_closure".to_string()));
        assert!(report
            .reasons
            .contains(&"missing_declarative_transitions".to_string()));
        assert!(report
            .solver
            .reason
            .contains("solver backends require machine IR"));
        assert!(report
            .solver
            .unsupported_features
            .contains(&"step(state, action)".to_string()));
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
            transition Step [tags = ["even_path"]] when |state| state.x < 3 && (state.x + 1) % 2 == 0 => [BranchState {
                x: state.x + 1,
                even: true,
            }];
            transition Step [tags = ["odd_path"]] when |state| state.x < 3 && (state.x + 1) % 2 != 0 => [BranchState {
                x: state.x + 1,
                even: false,
            }];
        }
        properties {
            invariant P_BRANCH_BOUND |state| state.x <= 3;
        }
    }

    valid_state! {
        struct DeadlockState {
            x: u8 [range = "0..=1"],
        }
    }

    valid_actions! {
        enum DeadlockAction {
            Advance => "ADVANCE" [reads = ["x"], writes = ["x"]],
        }
    }

    crate::valid_model! {
        model DeadlockFreedomFailModel<DeadlockState, DeadlockAction>;
        init [DeadlockState { x: 0 }];
        transitions {
            transition Advance when |state| state.x == 0 => [DeadlockState { x: 1 }];
        }
        properties {
            deadlock_freedom P_LIVE;
        }
    }

    crate::valid_model! {
        model DeadlockFreedomPassModel<DeadlockState, DeadlockAction>;
        init [DeadlockState { x: 0 }];
        transitions {
            transition Advance when |_state| true => [DeadlockState { x: 0 }];
        }
        properties {
            deadlock_freedom P_LIVE;
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
    fn declarative_set_models_report_solver_ready_capabilities() {
        let report = machine_capability_report::<RoleSetModel>();
        assert!(report.explicit_ready);
        assert!(report.ir_ready);
        assert!(report.solver_ready);
        assert!(report.reasons.is_empty());
    }

    #[cfg(feature = "varisat-backend")]
    #[test]
    fn varisat_matches_explicit_for_passing_finite_set_properties() {
        let explicit = check_machine_with_adapter::<RoleSetModel>(
            "req-role-set-pass-explicit",
            Some("P_ADMIN_IMPLIES_APPROVED"),
            &AdapterConfig::Explicit,
        )
        .expect("explicit backend should run");
        let varisat = check_machine_with_adapter::<RoleSetModel>(
            "req-role-set-pass-varisat",
            Some("P_ADMIN_IMPLIES_APPROVED"),
            &AdapterConfig::SatVarisat,
        )
        .expect("varisat backend should run");

        match (explicit, varisat) {
            (CheckOutcome::Completed(explicit), CheckOutcome::Completed(varisat)) => {
                assert_eq!(explicit.status, RunStatus::Pass);
                assert_eq!(varisat.status, explicit.status);
            }
            (explicit, varisat) => {
                panic!("unexpected outcomes: explicit={explicit:?}, varisat={varisat:?}")
            }
        }
    }

    #[cfg(feature = "varisat-backend")]
    #[test]
    fn varisat_matches_explicit_for_failing_finite_set_properties() {
        let explicit = check_machine_with_adapter::<RoleSetModel>(
            "req-role-set-fail-explicit",
            Some("P_APPROVED_IFF_NOT_EMPTY"),
            &AdapterConfig::Explicit,
        )
        .expect("explicit backend should run");
        let varisat = check_machine_with_adapter::<RoleSetModel>(
            "req-role-set-fail-varisat",
            Some("P_APPROVED_IFF_NOT_EMPTY"),
            &AdapterConfig::SatVarisat,
        )
        .expect("varisat backend should run");

        match (explicit, varisat) {
            (CheckOutcome::Completed(explicit), CheckOutcome::Completed(varisat)) => {
                let explicit_actions = explicit
                    .trace
                    .expect("explicit failure trace")
                    .steps
                    .iter()
                    .filter_map(|step| step.action_id.clone())
                    .flat_map(|action_ids| {
                        action_ids
                            .split(',')
                            .map(str::to_string)
                            .collect::<Vec<_>>()
                    })
                    .collect::<Vec<_>>();
                let varisat_actions = varisat
                    .trace
                    .expect("varisat failure trace")
                    .steps
                    .iter()
                    .filter_map(|step| step.action_id.clone())
                    .flat_map(|action_ids| {
                        action_ids
                            .split(',')
                            .map(str::to_string)
                            .collect::<Vec<_>>()
                    })
                    .collect::<Vec<_>>();
                assert_eq!(explicit.status, RunStatus::Fail);
                assert_eq!(varisat.status, explicit.status);
                assert_eq!(
                    explicit_actions,
                    vec!["GRANT_ADMIN".to_string(), "REVOKE_ADMIN".to_string()]
                );
                assert_eq!(varisat_actions, explicit_actions);
            }
            (explicit, varisat) => {
                panic!("unexpected outcomes: explicit={explicit:?}, varisat={varisat:?}")
            }
        }
    }

    #[test]
    fn duplicate_action_transitions_are_explored_and_lowered() {
        let result = check_machine::<BranchModel>();
        assert_eq!(result.status, ModelingRunStatus::Pass);
        assert_eq!(result.explored_states, 4);

        let lowered = lower_machine_model::<BranchModel>().expect("branch model lowers");
        assert_eq!(lowered.actions.len(), 2);
        let mut plan = RunPlan::default();
        plan.manifest = crate::engine::build_run_manifest(
            "req-branch".to_string(),
            "run-branch".to_string(),
            "sha256:test".to_string(),
            "sha256:test".to_string(),
            crate::engine::BackendKind::Explicit,
            env!("CARGO_PKG_VERSION").to_string(),
            Some(17),
        );
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

    #[test]
    fn rust_native_deadlock_freedom_can_fail() {
        let result = check_machine::<DeadlockFreedomFailModel>();
        assert_eq!(result.status, ModelingRunStatus::Fail);
        assert_eq!(result.trace.len(), 1);
    }

    #[test]
    fn rust_native_deadlock_freedom_can_pass() {
        let result = check_machine::<DeadlockFreedomPassModel>();
        assert_eq!(result.status, ModelingRunStatus::Pass);
    }

    #[test]
    fn rust_native_deadlock_freedom_lowers_to_ir() {
        let lowered =
            lower_machine_model::<DeadlockFreedomFailModel>().expect("deadlock model lowers");
        assert_eq!(lowered.properties.len(), 1);
        assert_eq!(
            lowered.properties[0].kind,
            crate::ir::PropertyKind::DeadlockFreedom
        );
        assert_eq!(
            lowered.properties[0].expr,
            ExprIr::Literal(Value::Bool(true))
        );
    }

    #[test]
    fn rust_native_deadlock_freedom_outcome_uses_deadlock_reason() {
        let outcome = check_machine_outcome::<DeadlockFreedomFailModel>("req-deadlock");
        match outcome {
            CheckOutcome::Completed(result) => {
                assert_eq!(
                    result.property_result.property_kind,
                    crate::ir::PropertyKind::DeadlockFreedom
                );
                assert_eq!(
                    result.property_result.reason_code.as_deref(),
                    Some("DEADLOCK_REACHED")
                );
                assert_eq!(result.status, RunStatus::Fail);
            }
            CheckOutcome::Errored(error) => panic!("unexpected error: {:?}", error.diagnostics),
        }
    }

    valid_state! {
        struct PasswordState {
            password: String [range = "0..=64"],
            password_set: bool,
            compliant: bool,
        }
    }

    valid_actions! {
        enum PasswordAction {
            SetStrongPassword => "SET_STRONG_PASSWORD" [reads = ["password_set"], writes = ["password", "password_set", "compliant"]],
            SetWeakPassword => "SET_WEAK_PASSWORD" [reads = ["password_set"], writes = ["password", "password_set", "compliant"]],
        }
    }

    crate::valid_model! {
        model PasswordPolicySafeModel<PasswordState, PasswordAction>;
        init [PasswordState {
            password: "".to_string(),
            password_set: false,
            compliant: false,
        }];
        transitions {
            transition SetStrongPassword [tags = ["password_policy_path", "allow_path"]] when |state| state.password_set == false
            => [PasswordState {
                password: "Str0ngPass!".to_string(),
                password_set: true,
                compliant: true,
            }];
        }
        properties {
            invariant P_PASSWORD_POLICY_MATCHES_FLAG |state|
                iff(
                    state.compliant,
                    state.password_set
                        && len(&state.password) >= 10
                        && regex_match(&state.password, r"[A-Z]")
                        && regex_match(&state.password, r"[a-z]")
                        && regex_match(&state.password, r"[0-9]")
                        && regex_match(&state.password, r"[^A-Za-z0-9]")
                );
        }
    }

    crate::valid_model! {
        model PasswordPolicyRegressionModel<PasswordState, PasswordAction>;
        init [PasswordState {
            password: "".to_string(),
            password_set: false,
            compliant: false,
        }];
        transitions {
            transition SetWeakPassword [tags = ["password_policy_path", "regression_path"]] when |state| state.password_set == false
            => [PasswordState {
                password: "password".to_string(),
                password_set: true,
                compliant: true,
            }];
        }
        properties {
            invariant P_PASSWORD_POLICY_MATCHES_FLAG |state|
                iff(
                    state.compliant,
                    state.password_set
                        && len(&state.password) >= 10
                        && regex_match(&state.password, r"[A-Z]")
                        && regex_match(&state.password, r"[a-z]")
                        && regex_match(&state.password, r"[0-9]")
                        && regex_match(&state.password, r"[^A-Za-z0-9]")
                );
        }
    }

    #[test]
    fn lower_machine_expr_supports_string_helpers() {
        let expr = lower_machine_expr(
            r#"len(&state.password) >= 10 && regex_match(&state.password, r"[A-Z]") && str_contains(&state.password, "!")"#,
        )
        .expect("string helper expression should lower");
        let debug = format!("{expr:?}");
        assert!(debug.contains("StringLen"));
        assert!(debug.contains("RegexMatch"));
        assert!(debug.contains("StringContains"));
    }

    #[test]
    fn declarative_string_models_are_explicit_ready_but_not_solver_ready() {
        let report = machine_capability_report::<PasswordPolicySafeModel>();
        assert!(report.explicit_ready);
        assert!(report.ir_ready);
        assert!(!report.solver_ready);
        assert!(report
            .solver
            .reason
            .contains("still needs the explicit backend"));
        assert!(report
            .solver
            .unsupported_features
            .contains(&"state field `password`: String".to_string()));
        assert!(report
            .solver
            .unsupported_features
            .contains(&"len(...)".to_string()));
        assert!(report
            .reasons
            .contains(&"string_fields_require_explicit_backend".to_string()));
        assert!(report
            .reasons
            .contains(&"regex_match_requires_explicit_backend".to_string()));
        assert!(report
            .solver
            .unsupported_features
            .contains(&"regex_match(...)".to_string()));

        let model = lower_machine_model::<PasswordPolicySafeModel>()
            .expect("password policy model should lower to machine IR");
        assert!(matches!(model.state_fields[0].ty, FieldType::String { .. }));
    }

    #[test]
    fn password_policy_models_can_pass_and_fail() {
        let safe = check_machine::<PasswordPolicySafeModel>();
        assert_eq!(safe.status, ModelingRunStatus::Pass);

        let regression = check_machine::<PasswordPolicyRegressionModel>();
        assert_eq!(regression.status, ModelingRunStatus::Fail);
        assert_eq!(regression.trace.len(), 1);
    }
}
