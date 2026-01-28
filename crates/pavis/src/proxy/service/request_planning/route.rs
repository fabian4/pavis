use crate::proxy::context::RouterContext;
use http::Uri;
use pavis_core::{PathMatch, Route};
use std::borrow::Cow;

pub fn apply_route_headers(ctx: &mut RouterContext, route: &Route) {
    ctx.request_headers = route.request_headers.clone();
    ctx.response_headers = route.response_headers.clone();
    ctx.route_timeout = route.timeout;
    ctx.retry_policy = route.retry.clone();
    ctx.retry_attempts = 0;
}

pub fn calculate_path_rewrite(
    route: &Route,
    uri_path: &str,
    uri_query: Option<&str>,
) -> Option<Uri> {
    match &route.rewrite.path {
        pavis_core::RewritePath::Disabled => None,
        pavis_core::RewritePath::Prefix { .. } => {
            let to = match &route.rewrite.path {
                pavis_core::RewritePath::Prefix { to, .. } => to,
                _ => unreachable!(),
            };

            let new_path = match &route.matcher.path {
                PathMatch::Prefix { path } => {
                    uri_path.strip_prefix(path.0.as_str()).map(|suffix| {
                        let mut path = String::with_capacity(to.0.len() + suffix.len());
                        path.push_str(&to.0);
                        path.push_str(suffix);
                        Cow::Owned(path)
                    })
                }
                PathMatch::Exact { path } => {
                    (uri_path == path.0.as_str()).then_some(Cow::Borrowed(to.0.as_str()))
                }
                PathMatch::Regex { .. } => None,
                #[allow(unreachable_patterns)]
                _ => None,
            };

            match new_path {
                Some(mut path) => {
                    if let Some(query) = uri_query {
                        let mut owned = match path {
                            Cow::Borrowed(path) => {
                                let mut owned = String::with_capacity(path.len() + 1 + query.len());
                                owned.push_str(path);
                                owned
                            }
                            Cow::Owned(mut owned) => {
                                owned.reserve(1 + query.len());
                                owned
                            }
                        };
                        owned.push('?');
                        owned.push_str(query);
                        path = Cow::Owned(owned);
                    }

                    match Uri::builder().path_and_query(path.as_ref()).build() {
                        Ok(uri) => Some(uri),
                        Err(err) => {
                            tracing::warn!(
                                error = %err,
                                rewrite = %to.0,
                                "Failed to apply path rewrite"
                            );
                            None
                        }
                    }
                }
                None => {
                    if matches!(route.matcher.path, PathMatch::Regex { .. }) {
                        tracing::warn!(
                            route = %route_path(route),
                            "Skipping path rewrite for regex match"
                        );
                    } else {
                        tracing::warn!(
                            route = %route_path(route),
                            path = %uri_path,
                            "Skipping path rewrite due to unmatched prefix"
                        );
                    }
                    None
                }
            }
        }
        #[allow(unreachable_patterns)]
        _ => None,
    }
}

pub fn route_path(route: &Route) -> &str {
    match &route.matcher.path {
        PathMatch::Prefix { path } => path.0.as_str(),
        PathMatch::Exact { path } => path.0.as_str(),
        PathMatch::Regex { path } => path.0.as_str(),
        #[allow(unreachable_patterns)]
        _ => "",
    }
}
