//! Route materialization and default resolution.
//!
//! This module implements Zero-Option enforcement for routes: it is responsible
//! for resolving all optional user inputs (e.g. missing matchers, default headers)
//! into concrete, explicit decisions before they reach pavis-core.

use super::semantic_validate::{parse_http_method, to_runtime_header_predicate};
use crate::config::types::{HeaderOperations, HeaderPredicate, Matcher, PathMatcher};
use anyhow::Result;
use pavis_core::{
    HeaderName, HeaderPredicates, HeaderValue, Headers, HeadersPolicy, MethodPredicate, PathMatch,
    RouteMatcher,
};

pub fn to_runtime_matcher(
    path: PathMatch,
    method: Option<String>,
    methods: Option<Vec<String>>,
    headers: Option<Vec<HeaderPredicate>>,
    vhost_index: usize,
    path_index: usize,
) -> Result<RouteMatcher> {
    let method_field_path = route_method_field_path(vhost_index, path_index);

    let method = if let Some(list) = methods {
        let mut core_list = Vec::with_capacity(list.len());
        for (i, m) in list.into_iter().enumerate() {
            let path = format!("{}[{}]", method_field_path, i);
            core_list.push(parse_http_method(&m, path)?);
        }
        MethodPredicate::List(core_list)
    } else if let Some(m) = method {
        let http_method = parse_http_method(&m, method_field_path)?;
        MethodPredicate::Specific(http_method)
    } else {
        MethodPredicate::Any
    };

    let headers = match headers {
        None => HeaderPredicates::None,
        Some(preds) if preds.is_empty() => HeaderPredicates::None,
        Some(preds) => {
            let core_preds = preds
                .into_iter()
                .enumerate()
                .map(|(header_index, predicate)| {
                    to_runtime_header_predicate(predicate, vhost_index, path_index, header_index)
                })
                .collect::<Result<Vec<_>>>()?;
            HeaderPredicates::Some(core_preds)
        }
    };

    Ok(RouteMatcher {
        path,
        method,
        headers,
    })
}

pub fn to_runtime_headers(h: Option<HeaderOperations>) -> HeadersPolicy {
    match h {
        None => HeadersPolicy::Disabled,
        Some(h) => HeadersPolicy::Enabled {
            rules: Headers {
                set_headers: h
                    .set_headers
                    .into_iter()
                    .map(|(k, v)| (HeaderName(k), HeaderValue(v)))
                    .collect(),
                append_headers: h
                    .append_headers
                    .into_iter()
                    .map(|(k, v)| (HeaderName(k), HeaderValue(v)))
                    .collect(),
                add_headers: h
                    .add_headers
                    .into_iter()
                    .map(|(k, v)| (HeaderName(k), HeaderValue(v)))
                    .collect(),
                remove_headers: h.remove_headers.into_iter().map(HeaderName).collect(),
            },
        },
    }
}

pub fn default_matcher() -> Matcher {
    Matcher {
        path: PathMatcher::Prefix {
            path: "/".to_string(),
        },
        method: None,
        methods: None,
        headers: None,
    }
}

pub fn matcher_path(matcher: &Matcher) -> String {
    match &matcher.path {
        PathMatcher::Prefix { path } => path.clone(),
        PathMatcher::Exact { path } => path.clone(),
        PathMatcher::Regex { path } => path.clone(),
    }
}

fn route_match_field_path(vhost_index: usize, path_index: usize) -> pavis_core::FieldPathBuilder {
    pavis_core::FieldPathBuilder::new()
        .root("routes")
        .index(vhost_index)
        .field("paths")
        .index(path_index)
        .field("match")
}

fn route_method_field_path(vhost_index: usize, path_index: usize) -> String {
    route_match_field_path(vhost_index, path_index)
        .field("method")
        .finish()
}
