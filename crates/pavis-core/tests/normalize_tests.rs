//! Normalization tests for PredicateNode

use compact_str::CompactString;
use pavis_core::runtime::HttpMethod;
use pavis_core::runtime::matcher::*;

#[test]
fn test_flatten_nested_and() {
    let pred = PredicateNode::And(vec![
        PredicateNode::And(vec![
            PredicateNode::True,
            PredicateNode::Method(MethodMatcher::Exact(HttpMethod::GET)),
        ]),
        PredicateNode::True,
    ]);

    let normalized = pred.normalize();
    assert_eq!(
        normalized,
        PredicateNode::Method(MethodMatcher::Exact(HttpMethod::GET))
    );
}

#[test]
fn test_flatten_nested_or() {
    let pred = PredicateNode::Or(vec![
        PredicateNode::Or(vec![
            PredicateNode::False,
            PredicateNode::Method(MethodMatcher::Exact(HttpMethod::POST)),
        ]),
        PredicateNode::False,
    ]);

    let normalized = pred.normalize();
    assert_eq!(
        normalized,
        PredicateNode::Method(MethodMatcher::Exact(HttpMethod::POST))
    );
}

#[test]
fn test_remove_identity_and() {
    let pred = PredicateNode::And(vec![
        PredicateNode::Header(HeaderMatcher::Exact {
            name: CompactString::new("x-foo"),
            value: CompactString::new("bar"),
        }),
        PredicateNode::True,
        PredicateNode::Header(HeaderMatcher::Exact {
            name: CompactString::new("x-baz"),
            value: CompactString::new("qux"),
        }),
    ]);

    let normalized = pred.normalize();
    if let PredicateNode::And(children) = normalized {
        assert_eq!(children.len(), 2);
        // True should be removed
        assert!(children.iter().all(|n| !matches!(n, PredicateNode::True)));
    } else {
        panic!("Expected And node");
    }
}

#[test]
fn test_remove_identity_or() {
    let pred = PredicateNode::Or(vec![
        PredicateNode::Header(HeaderMatcher::Exact {
            name: CompactString::new("x-foo"),
            value: CompactString::new("bar"),
        }),
        PredicateNode::False,
        PredicateNode::Header(HeaderMatcher::Exact {
            name: CompactString::new("x-baz"),
            value: CompactString::new("qux"),
        }),
    ]);

    let normalized = pred.normalize();
    if let PredicateNode::Or(children) = normalized {
        assert_eq!(children.len(), 2);
        // False should be removed
        assert!(children.iter().all(|n| !matches!(n, PredicateNode::False)));
    } else {
        panic!("Expected Or node");
    }
}

#[test]
fn test_simplify_single_and() {
    let pred = PredicateNode::And(vec![PredicateNode::Method(MethodMatcher::Exact(
        HttpMethod::GET,
    ))]);

    let normalized = pred.normalize();
    assert_eq!(
        normalized,
        PredicateNode::Method(MethodMatcher::Exact(HttpMethod::GET))
    );
}

#[test]
fn test_simplify_empty_and() {
    let pred = PredicateNode::And(vec![]);
    assert_eq!(pred.normalize(), PredicateNode::True);
}

#[test]
fn test_simplify_empty_or() {
    let pred = PredicateNode::Or(vec![]);
    assert_eq!(pred.normalize(), PredicateNode::False);
}

#[test]
fn test_double_negation() {
    let pred = PredicateNode::Not(Box::new(PredicateNode::Not(Box::new(
        PredicateNode::Method(MethodMatcher::Exact(HttpMethod::GET)),
    ))));

    assert_eq!(
        pred.normalize(),
        PredicateNode::Method(MethodMatcher::Exact(HttpMethod::GET))
    );
}

#[test]
fn test_negate_true() {
    let pred = PredicateNode::Not(Box::new(PredicateNode::True));
    assert_eq!(pred.normalize(), PredicateNode::False);
}

#[test]
fn test_negate_false() {
    let pred = PredicateNode::Not(Box::new(PredicateNode::False));
    assert_eq!(pred.normalize(), PredicateNode::True);
}

#[test]
fn test_stable_sorting() {
    let pred = PredicateNode::And(vec![
        PredicateNode::Header(HeaderMatcher::Regex {
            name: CompactString::new("x-version"),
            pattern: CompactString::new("v.*"),
        }),
        PredicateNode::Header(HeaderMatcher::Exact {
            name: CompactString::new("x-tenant"),
            value: CompactString::new("alice"),
        }),
    ]);

    let normalized = pred.normalize();
    // Exact (cost 1) should come before Regex (cost 10)
    if let PredicateNode::And(children) = normalized {
        assert_eq!(children.len(), 2);
        assert!(matches!(
            children[0],
            PredicateNode::Header(HeaderMatcher::Exact { .. })
        ));
        assert!(matches!(
            children[1],
            PredicateNode::Header(HeaderMatcher::Regex { .. })
        ));
    } else {
        panic!("Expected And node");
    }
}

#[test]
fn test_stable_sorting_twice() {
    let pred = PredicateNode::And(vec![
        PredicateNode::Header(HeaderMatcher::Regex {
            name: CompactString::new("x-version"),
            pattern: CompactString::new("v.*"),
        }),
        PredicateNode::Header(HeaderMatcher::Exact {
            name: CompactString::new("x-tenant"),
            value: CompactString::new("alice"),
        }),
    ]);

    let normalized1 = pred.normalize();
    let normalized2 = normalized1.clone().normalize();

    // Normalizing twice should produce identical result
    assert_eq!(normalized1, normalized2);
}

#[test]
fn test_deduplicate() {
    let pred = PredicateNode::And(vec![
        PredicateNode::Header(HeaderMatcher::Exact {
            name: CompactString::new("x-foo"),
            value: CompactString::new("bar"),
        }),
        PredicateNode::Header(HeaderMatcher::Exact {
            name: CompactString::new("x-foo"),
            value: CompactString::new("bar"),
        }),
    ]);

    let normalized = pred.normalize();
    assert_eq!(
        normalized,
        PredicateNode::Header(HeaderMatcher::Exact {
            name: CompactString::new("x-foo"),
            value: CompactString::new("bar"),
        })
    );
}

#[test]
fn test_absorbing_false_in_and() {
    let pred = PredicateNode::And(vec![
        PredicateNode::Method(MethodMatcher::Exact(HttpMethod::GET)),
        PredicateNode::False,
        PredicateNode::Header(HeaderMatcher::Present {
            name: CompactString::new("x-foo"),
        }),
    ]);

    let normalized = pred.normalize();
    // False is absorbing element for AND
    assert_eq!(normalized, PredicateNode::False);
}

#[test]
fn test_absorbing_true_in_or() {
    let pred = PredicateNode::Or(vec![
        PredicateNode::Method(MethodMatcher::Exact(HttpMethod::GET)),
        PredicateNode::True,
        PredicateNode::Header(HeaderMatcher::Present {
            name: CompactString::new("x-foo"),
        }),
    ]);

    let normalized = pred.normalize();
    // True is absorbing element for OR
    assert_eq!(normalized, PredicateNode::True);
}

#[test]
fn test_p0_backward_compat() {
    // P0 style: And([Method(GET), Header(exact)])
    let pred = PredicateNode::And(vec![
        PredicateNode::Method(MethodMatcher::Exact(HttpMethod::GET)),
        PredicateNode::Header(HeaderMatcher::Exact {
            name: CompactString::new("x-tenant"),
            value: CompactString::new("alice"),
        }),
    ]);

    let normalized = pred.normalize();

    // Should remain an And with 2 children, just sorted
    if let PredicateNode::And(children) = normalized {
        assert_eq!(children.len(), 2);
        // Both predicates should be present
        assert!(
            children
                .iter()
                .any(|n| matches!(n, PredicateNode::Method(_)))
        );
        assert!(
            children
                .iter()
                .any(|n| matches!(n, PredicateNode::Header(_)))
        );
    } else {
        panic!("Expected And node for P0 compatibility");
    }
}

#[test]
fn test_cost_ordering_multiple_operators() {
    let pred = PredicateNode::And(vec![
        PredicateNode::Header(HeaderMatcher::Regex {
            name: CompactString::new("x-version"),
            pattern: CompactString::new("v[0-9]+"),
        }),
        PredicateNode::Header(HeaderMatcher::Prefix {
            name: CompactString::new("x-tenant"),
            prefix: CompactString::new("team-"),
        }),
        PredicateNode::Header(HeaderMatcher::Exact {
            name: CompactString::new("x-env"),
            value: CompactString::new("prod"),
        }),
        PredicateNode::Header(HeaderMatcher::Present {
            name: CompactString::new("x-debug"),
        }),
    ]);

    let normalized = pred.normalize();

    if let PredicateNode::And(children) = normalized {
        // Cost ordering: exact(1), present(1), prefix(2), regex(10)
        // Exact and Present tie on cost, so lexicographic sort applies
        let costs: Vec<u8> = children
            .iter()
            .map(|n| match n {
                PredicateNode::Header(h) => h.cost().0,
                _ => 0,
            })
            .collect();

        // Should be sorted by cost
        for i in 1..costs.len() {
            assert!(costs[i - 1] <= costs[i], "Costs not sorted: {:?}", costs);
        }
    } else {
        panic!("Expected And node");
    }
}

#[test]
fn test_complex_nested_normalization() {
    let pred = PredicateNode::Or(vec![
        PredicateNode::And(vec![
            PredicateNode::And(vec![
                PredicateNode::True,
                PredicateNode::Method(MethodMatcher::Exact(HttpMethod::GET)),
            ]),
            PredicateNode::True,
        ]),
        PredicateNode::Or(vec![
            PredicateNode::False,
            PredicateNode::Header(HeaderMatcher::Present {
                name: CompactString::new("x-foo"),
            }),
        ]),
    ]);

    let normalized = pred.normalize();

    // Should flatten to Or([Method(GET), Header(Present)])
    if let PredicateNode::Or(children) = normalized {
        assert_eq!(children.len(), 2);
        assert!(
            children
                .iter()
                .any(|n| matches!(n, PredicateNode::Method(_)))
        );
        assert!(
            children
                .iter()
                .any(|n| matches!(n, PredicateNode::Header(_)))
        );
    } else {
        panic!("Expected Or node");
    }
}

#[test]
fn test_cost_capping() {
    let mut children = Vec::new();
    for i in 0..30 {
        children.push(PredicateNode::Header(HeaderMatcher::Regex {
            name: CompactString::from(format!("x-foo-{}", i)),
            pattern: CompactString::from(".*"),
        }));
    }
    // Each regex has cost 10. 30 * 10 = 300. Should be capped at 255.
    let pred = PredicateNode::And(children);
    assert_eq!(pred.cost().0, 255);
}

#[test]
fn test_not_node_cost() {
    let inner = PredicateNode::Header(HeaderMatcher::Regex {
        name: CompactString::new("x-foo"),
        pattern: CompactString::new(".*"),
    });
    let cost = inner.cost();
    let pred = PredicateNode::Not(Box::new(inner));
    assert_eq!(pred.cost(), cost);
}
