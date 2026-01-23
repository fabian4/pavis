//! Matcher types for advanced routing predicates (P2)
//!
//! This module defines the predicate AST used for route matching with support for
//! method matchers, header matchers, and logical combinations (And/Or/Not).
//!
//! ## Normalization
//!
//! PredicateNode provides a `normalize()` method that applies 5 deterministic rules:
//! 1. Flatten nested And/Or nodes
//! 2. Remove identity elements (True from And, False from Or)
//! 3. Simplify trivial cases (empty And → True, empty Or → False, single-child → child)
//! 4. Sort children by evaluation cost (cheaper predicates first)
//! 5. Deduplicate identical predicates
//!
//! Normalization is idempotent and deterministic, enabling structural equality checks.

use compact_str::CompactString;
use rkyv::{Archive, Deserialize as RkyvDeserialize, Serialize as RkyvSerialize};

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::runtime::HttpMethod;

/// Cost estimate for predicate evaluation (optimization hint, not semantic)
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Archive,
    RkyvDeserialize,
    RkyvSerialize,
    bytecheck::CheckBytes,
)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[rkyv(compare(PartialEq))]
pub struct MatcherCost(pub u8);

/// Method matcher variants
#[derive(
    Debug, Clone, PartialEq, Eq, Archive, RkyvDeserialize, RkyvSerialize, bytecheck::CheckBytes,
)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[rkyv(compare(PartialEq))]
#[rkyv(attr(derive(Debug)))]
#[repr(u8)]
pub enum MethodMatcher {
    /// Match a single HTTP method
    Exact(HttpMethod),
    /// Match any of the specified methods
    AnyOf(Vec<HttpMethod>),
}

impl MethodMatcher {
    pub fn cost(&self) -> MatcherCost {
        MatcherCost(1) // Method matching is always cheap
    }
}

/// Header matcher variants
#[derive(
    Debug, Clone, PartialEq, Eq, Archive, RkyvDeserialize, RkyvSerialize, bytecheck::CheckBytes,
)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[rkyv(compare(PartialEq))]
#[rkyv(attr(derive(Debug)))]
#[repr(u8)]
pub enum HeaderMatcher {
    /// Exact header value match
    Exact {
        name: CompactString,
        value: CompactString,
    },
    /// Header value prefix match
    Prefix {
        name: CompactString,
        prefix: CompactString,
    },
    /// Header value regex match (pattern is stored as string, compiled at runtime)
    Regex {
        name: CompactString,
        pattern: CompactString,
    },
    /// Header presence check (value doesn't matter)
    Present { name: CompactString },
}

impl HeaderMatcher {
    pub fn cost(&self) -> MatcherCost {
        match self {
            HeaderMatcher::Exact { .. } => MatcherCost(1),
            HeaderMatcher::Present { .. } => MatcherCost(1),
            HeaderMatcher::Prefix { .. } => MatcherCost(2),
            HeaderMatcher::Regex { .. } => MatcherCost(10),
        }
    }
}

/// Predicate AST node for route matching
///
/// Note: rkyv serialization will be added when needed for .pvs artifacts
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum PredicateNode {
    /// Always matches
    True,
    /// Never matches
    False,
    /// Method predicate
    Method(MethodMatcher),
    /// Header predicate
    Header(HeaderMatcher),
    /// Logical AND (all children must match)
    And(Vec<PredicateNode>),
    /// Logical OR (at least one child must match)
    Or(Vec<PredicateNode>),
    /// Logical NOT (child must not match)
    Not(Box<PredicateNode>),
}

impl PredicateNode {
    /// Normalize this predicate tree to canonical form
    ///
    /// Applies 5 rules:
    /// 1. Flatten nested And/Or
    /// 2. Remove identity elements
    /// 3. Simplify trivial cases
    /// 4. Sort by cost
    /// 5. Deduplicate
    ///
    /// Normalization is idempotent: `p.normalize().normalize() == p.normalize()`
    pub fn normalize(self) -> Self {
        match self {
            PredicateNode::And(children) => Self::normalize_and(children),
            PredicateNode::Or(children) => Self::normalize_or(children),
            PredicateNode::Not(child) => Self::normalize_not(*child),
            other => other, // True, False, Method, Header are already canonical
        }
    }

    fn normalize_and(children: Vec<PredicateNode>) -> Self {
        // Step 1: Recursively normalize all children
        let normalized: Vec<PredicateNode> = children.into_iter().map(|c| c.normalize()).collect();

        // Step 2: Flatten nested And nodes
        let mut flattened = Vec::new();
        for child in normalized {
            match child {
                PredicateNode::And(inner) => flattened.extend(inner),
                other => flattened.push(other),
            }
        }

        // Step 3: Check for absorbing element (False)
        if flattened.iter().any(|n| matches!(n, PredicateNode::False)) {
            return PredicateNode::False;
        }

        // Step 4: Remove identity element (True)
        flattened.retain(|n| !matches!(n, PredicateNode::True));

        // Step 5: Sort by cost (stable sort to preserve order for equal costs)
        flattened.sort_by_key(|n| n.cost());

        // Step 6: Deduplicate
        flattened.dedup();

        // Step 7: Simplify trivial cases
        match flattened.len() {
            0 => PredicateNode::True,
            1 => flattened.into_iter().next().unwrap(),
            _ => PredicateNode::And(flattened),
        }
    }

    fn normalize_or(children: Vec<PredicateNode>) -> Self {
        // Step 1: Recursively normalize all children
        let normalized: Vec<PredicateNode> = children.into_iter().map(|c| c.normalize()).collect();

        // Step 2: Flatten nested Or nodes
        let mut flattened = Vec::new();
        for child in normalized {
            match child {
                PredicateNode::Or(inner) => flattened.extend(inner),
                other => flattened.push(other),
            }
        }

        // Step 3: Check for absorbing element (True)
        if flattened.iter().any(|n| matches!(n, PredicateNode::True)) {
            return PredicateNode::True;
        }

        // Step 4: Remove identity element (False)
        flattened.retain(|n| !matches!(n, PredicateNode::False));

        // Step 5: Sort by cost (stable sort to preserve order for equal costs)
        flattened.sort_by_key(|n| n.cost());

        // Step 6: Deduplicate
        flattened.dedup();

        // Step 7: Simplify trivial cases
        match flattened.len() {
            0 => PredicateNode::False,
            1 => flattened.into_iter().next().unwrap(),
            _ => PredicateNode::Or(flattened),
        }
    }

    fn normalize_not(child: PredicateNode) -> Self {
        let normalized = child.normalize();
        match normalized {
            PredicateNode::True => PredicateNode::False,
            PredicateNode::False => PredicateNode::True,
            PredicateNode::Not(inner) => *inner, // Double negation
            other => PredicateNode::Not(Box::new(other)),
        }
    }

    /// Compute evaluation cost estimate (for sorting optimization)
    pub fn cost(&self) -> MatcherCost {
        match self {
            PredicateNode::True => MatcherCost(0),
            PredicateNode::False => MatcherCost(0),
            PredicateNode::Method(m) => m.cost(),
            PredicateNode::Header(h) => h.cost(),
            PredicateNode::And(children) | PredicateNode::Or(children) => {
                // Sum of children costs
                let sum: u16 = children.iter().map(|c| c.cost().0 as u16).sum();
                MatcherCost(sum.min(255) as u8) // Cap at 255
            }
            PredicateNode::Not(child) => child.cost(),
        }
    }
}
