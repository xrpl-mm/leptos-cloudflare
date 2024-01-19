use std::collections::HashSet;

use futures::{Stream, StreamExt};
use leptos::leptos_server::server_fn_by_path;
use leptos::server_fn::{Encoding, Payload};
use leptos::{create_runtime, provide_context};
use leptos::{
    ssr::render_to_stream_with_prefix_undisposed_with_context_and_block_replacement, use_context,
    IntoView, LeptosOptions, RuntimeId, View,
};
use leptos_integration_utils::{build_async_response, html_parts_separated};
use leptos_meta::{generate_head_metadata_separated, MetaContext};
use leptos_router::{provide_server_redirect, RouteListing, SsrMode};
use leptos_router::{Method as LeptosMethod, RouterIntegrationContext, ServerIntegration};

use worker::Headers;

pub trait LeptosRoutes {
    fn leptos_routes(self, paths: Vec<RouteListing>) -> Self;
}

/// This is the information about the original Request from Cloudflare worker.
/// The request is meant to be inserted as an argument to (provide_context)[leptos::provide_context],
/// but [worker::Request](worker::Request) doesn't implement Clone, so we need to wrap it in a struct.
#[derive(Debug, Clone)]
pub struct RequestParts {
    pub body: Vec<u8>,
    pub method: worker::Method,
    pub headers: worker::Headers,
    pub url: worker::Url,
    pub edge_request: Result<web_sys::Request, wasm_bindgen::JsValue>,
}

/// This struct lets you define headers and override the status of the Response from an Element or a Server Function
/// Typically contained inside of a ResponseOptions. Setting this is useful for cookies and custom responses.
#[derive(Debug, Clone)]
pub struct ResponseOptions {
    pub status: Option<u16>,
    pub headers: worker::Headers,
}

/// Cloudflare Worker handler can only access variables from [RouterContext](worker::RouteContext). Therefore,
/// we want to put all the variables we need in route handler into this struct.
#[derive(Clone)]
pub struct WorkerRouterData<IV, AppFn>
where
    IV: IntoView + 'static,
    AppFn: Fn() -> IV + Clone + Send + 'static,
{
    pub options: LeptosOptions,
    /// A set of local directories that should serve static assets from the KV store.
    pub app_fn: AppFn,
}

pub async fn generate_request_parts(req: &mut worker::Request) -> worker::Result<RequestParts> {
    let body = req.bytes().await.unwrap_or_default();
    let method = req.method();
    let headers = req.headers().clone();
    let edge_request = req.inner();
    let url = req.url()?;

    Ok(RequestParts {
        method,
        url,
        body,
        edge_request: edge_request.clone(),
        headers,
    })
}

/// Provides an easy way to redirect the user from within a server function. Mimicking the Remix `redirect()`,
/// it sets a [StatusCode] of 302 and a [LOCATION](header::LOCATION) header with the provided value.
/// If looking to redirect from the client, `leptos_router::use_navigate()` should be used instead.
#[tracing::instrument(level = "trace", fields(error), skip_all)]
pub fn redirect(path: &str) {
    if let Some(mut response_options) = use_context::<ResponseOptions>() {
        response_options.status = Some(302);
        response_options
            .insert_header("location", path)
            .expect("failed to insert header value");
    }
}

#[tracing::instrument(level = "trace", fields(error), skip_all)]
pub async fn handle_server_fns<IV, AppFn>(
    mut req: worker::Request,
    _ctx: worker::RouteContext<WorkerRouterData<IV, AppFn>>,
) -> worker::Result<worker::Response>
where
    IV: IntoView + 'static,
    AppFn: Fn() -> IV + Clone + Send + 'static,
{
    let url = req.url()?;
    let path_segments = url.path_segments();
    let path_segments = match path_segments {
        Some(path_segments) => path_segments,
        None => return worker::Response::error("Server functions cannot be hosted at root /", 500),
    };

    // last element must exist, since we already checked that path_segments is not empty
    let api_path = path_segments.last().unwrap();

    if let Some(server_fn) = server_fn_by_path(api_path) {
        let runtime = create_runtime();

        let req_parts = generate_request_parts(&mut req).await?;
        provide_context(req_parts.clone());
        // Add this so that we can set headers and status of the response
        provide_context(ResponseOptions::default());

        let query_bytes = &url.query().unwrap_or("").as_bytes();

        let data = match &server_fn.encoding() {
            Encoding::Url | Encoding::Cbor => req_parts.body.as_slice(),
            Encoding::GetJSON | Encoding::GetCBOR => query_bytes,
        };

        let response = match server_fn.call((), data).await {
            Ok(serialized) => {
                // If ResponseOptions are set, add the headers and status to the request
                let res_options = use_context::<ResponseOptions>();
                let accept_header = match req.headers().get("Accept") {
                    Ok(accept_header) => accept_header,
                    Err(_) => return worker::Response::error("Accept header not found", 500),
                };

                let mut status: u16 = 200;
                let mut headers = res_options.clone().unwrap().headers;

                if accept_header == Some("application/json".to_string())
                    || accept_header
                        == Some(
                            "application/\
                                                 x-www-form-urlencoded"
                                .to_string(),
                        )
                    || accept_header == Some("application/cbor".to_string())
                {
                }
                // otherwise, it's probably a <form> submit or something: redirect back to the referrer
                else {
                    let referer = match req.headers().get("Referer") {
                        Ok(referer) => referer.unwrap_or("/".to_string()),
                        Err(_) => return worker::Response::error("Referer header not found", 500),
                    };

                    match headers.set("Location", &referer) {
                        Ok(_) => (),
                        Err(_) => {
                            return worker::Response::error("Failed to set Location header", 500)
                        }
                    };
                }

                let overriding_status = &res_options.unwrap().status;
                match overriding_status {
                    Some(overriding_status) => status = *overriding_status,
                    None => {}
                };
                match serialized {
                    Payload::Binary(data) => {
                        // append only throws when the header key is invalid
                        // but it's not, so we can unwrap
                        headers.append("content-type", "application/cbor").unwrap();
                        return Ok(
                            worker::Response::from_body(worker::ResponseBody::Body(data))?
                                .with_status(status)
                                .with_headers(headers),
                        );
                    }
                    Payload::Url(data) => {
                        // append only throws when the header key is invalid
                        // but it's not, so we can unwrap
                        headers
                            .append("content-type", "application/x-www-form-urlencoded")
                            .unwrap();
                        return Ok(worker::Response::from_body(worker::ResponseBody::Body(
                            data.as_bytes().to_vec(),
                        ))?
                        .with_status(status)
                        .with_headers(headers));
                    }
                    Payload::Json(data) => {
                        // append only throws when the header key is invalid
                        // but it's not, so we can unwrap
                        headers.append("content-type", "application/json").unwrap();
                        return Ok(worker::Response::from_body(worker::ResponseBody::Body(
                            data.as_bytes().to_vec(),
                        ))?
                        .with_status(status)
                        .with_headers(headers));
                    }
                }
            }
            Err(err) => {
                worker::Response::from_bytes(err.to_string().as_bytes().to_vec())?.with_status(500)
            }
        };
        // clean up the scope
        runtime.dispose();

        Ok(response)
    } else {
        let response = worker::Response::from_bytes(
            format!(
                "Could not find a server function at the \
                 route {api_path}. \n\nIt's likely that \
                 either 
                 1. The API prefix you specify in the \
                 `#[server]` macro doesn't match the \
                 prefix at which your server function \
                 handler is mounted, or \n2. You are on a \
                 platform that doesn't support automatic \
                 server function registration and you \
                 need to call \
                 ServerFn::register_explicit() on the \
                 server function type, somewhere in your \
                 `main` function.",
            )
            .into(),
        )
        // The only possible error is setting the header inside this function
        // as octet-stream, and it should never go wrong
        .unwrap()
        .with_status(400);

        Ok(response)
    }
}

/// Serves the static assets from the Cloudflare site's directory.
/// These assets will be served by Cloudflare's KV Store.
// pub async fn serve_static_from_kv<IV, AppFn>(
//     req: worker::Request,
//     ctx: worker::RouteContext<WorkerRouterData<IV, AppFn>>,
// ) -> worker::Result<worker::Response>
// where
//     IV: IntoView + 'static,
//     AppFn: Fn() -> IV + Clone + Send + 'static,
// {
//     let url = req.url();
//     let asset_key = url
//         .as_ref()
//         .ok()
//         .and_then(|url| url.path_segments())
//         .and_then(|mut path_segments| {
//             path_segments.next().and_then(|pkg_dir| {
//                 if pkg_dir == ctx.data.options.site_pkg_dir
//                     || ctx.data.static_dirs.contains(pkg_dir)
//                 {
//                     path_segments.next()
//                 } else {
//                     None
//                 }
//             })
//         });

//     let asset_key = match asset_key {
//         Some(asset_key) => asset_key,
//         None => return worker::Response::error("Not found", 404),
//     };
//     let store = ctx.env.kv("__STATIC_CONTENT")?;
//     let file_path = match ctx.env.asset_key(asset_key) {
//         Ok(file_path) => file_path,
//         Err(_) => return worker::Response::error("Not found", 404),
//     };

//     if let Some(bytes) = store.get(&file_path).bytes().await? {
//         let mut response = worker::Response::from_bytes(bytes)?;
//         let content_type = match mime_guess::from_path(file_path).first() {
//             Some(content_type) => content_type,
//             None => return worker::Response::error("Unsupported file type", 415),
//         };
//         response
//             .headers_mut()
//             .set("Content-Type", content_type.essence_str())?;
//         Ok(response)
//     } else {
//         worker::Response::error("Not found", 404)
//     }
// }

#[tracing::instrument(level = "trace", fields(error), skip_all)]
pub fn render_app_to_stream_with_context<'a, 'b, IV, AppFn>(
    method: LeptosMethod,
    path: &'a str,
    cf_router: worker::Router<'b, WorkerRouterData<IV, AppFn>>,
) -> worker::Router<'b, WorkerRouterData<IV, AppFn>>
where
    IV: IntoView + 'static,
    AppFn: Fn() -> IV + Clone + Send + 'static,
{
    let handler = |mut req: worker::Request,
                   ctx: worker::RouteContext<WorkerRouterData<IV, AppFn>>| async move {
        let options = ctx.data.options;
        let app_fn = ctx.data.app_fn;
        let res_options = ResponseOptions::default();
        let app = {
            let app_fn = app_fn.clone();
            let res_options = res_options.clone();

            let request_parts = generate_request_parts(&mut req).await?;

            move || {
                provide_contexts(request_parts.url.to_string(), request_parts, res_options);
                (app_fn)().into_view()
            }
        };

        stream_app(&options, app, res_options, || {}, false).await
    };

    match method {
        LeptosMethod::Get => cf_router.get_async(path, handler),
        LeptosMethod::Post => cf_router.post_async(path, handler),
        LeptosMethod::Put => cf_router.put_async(path, handler),
        LeptosMethod::Delete => cf_router.delete_async(path, handler),
        LeptosMethod::Patch => cf_router.patch_async(path, handler),
    }
}

/// Variation of [render_app_to_stream_with_context](render_app_to_stream_with_context) in which
/// only `replace_blocks` parameter of [stream_app](stream_app) is set to true.
///
/// The reason that it cannot be made into a single function is that the `replace_blocks` parameter
/// cannot be used inside the handler, since everything in the handler should be accessed from the
/// the handler's context.
#[tracing::instrument(level = "trace", fields(error), skip_all)]
pub fn render_app_to_stream_with_context_and_replace_blocks<'a, 'b, IV, AppFn>(
    method: LeptosMethod,
    path: &'a str,
    cf_router: worker::Router<'b, WorkerRouterData<IV, AppFn>>,
) -> worker::Router<'b, WorkerRouterData<IV, AppFn>>
where
    IV: IntoView + 'static,
    AppFn: Fn() -> IV + Clone + Send + 'static,
{
    let handler = |mut req: worker::Request,
                   ctx: worker::RouteContext<WorkerRouterData<IV, AppFn>>| async move {
        let options = ctx.data.options;
        let app_fn = ctx.data.app_fn;
        let res_options = ResponseOptions::default();
        let app = {
            let app_fn = app_fn.clone();
            let res_options = res_options.clone();

            let request_parts = generate_request_parts(&mut req).await?;

            move || {
                provide_contexts(request_parts.url.to_string(), request_parts, res_options);
                (app_fn)().into_view()
            }
        };

        stream_app(&options, app, res_options, || {}, true).await
    };

    match method {
        LeptosMethod::Get => cf_router.get_async(path, handler),
        LeptosMethod::Post => cf_router.post_async(path, handler),
        LeptosMethod::Put => cf_router.put_async(path, handler),
        LeptosMethod::Delete => cf_router.delete_async(path, handler),
        LeptosMethod::Patch => cf_router.patch_async(path, handler),
    }
}

/// Generates a list of all routes defined in Leptos's Router in your app. We can then use this to automatically
/// create routes in Actix's App without having to use wildcard matching or fallbacks. Takes in your root app Element
/// as an argument so it can walk you app tree. This version is tailored to generated Actix compatible paths.
pub fn generate_route_list<IV>(app_fn: impl Fn() -> IV + 'static + Clone) -> Vec<RouteListing>
where
    IV: IntoView + 'static,
{
    generate_route_list_with_exclusions(app_fn, None)
}

/// Generates a list of all routes defined in Leptos's Router in your app. We can then use this to automatically
/// create routes in Actix's App without having to use wildcard matching or fallbacks. Takes in your root app Element
/// as an argument so it can walk you app tree. This version is tailored to generated Actix compatible paths. Adding excluded_routes
/// to this function will stop `.leptos_routes()` from generating a route for it, allowing a custom handler. These need to be in Actix path format
pub fn generate_route_list_with_exclusions<IV>(
    app_fn: impl Fn() -> IV + 'static + Clone,
    excluded_routes: Option<Vec<String>>,
) -> Vec<RouteListing>
where
    IV: IntoView + 'static,
{
    let (mut routes, static_data_map) = leptos_router::generate_route_list_inner(app_fn);

    // Empty strings screw with Actix pathing, they need to be "/"
    routes = routes
        .into_iter()
        .map(|listing| {
            let path = listing.path();
            if path.is_empty() {
                RouteListing::new(
                    "/".to_string(),
                    listing.path(),
                    listing.mode(),
                    listing.methods(),
                    listing.static_mode(),
                )
            } else {
                listing
            }
        })
        .collect::<Vec<_>>();

    if routes.is_empty() {
        vec![RouteListing::new(
            "/",
            "",
            Default::default(),
            [leptos_router::Method::Get],
            None,
        )]
    } else {
        // Routes to exclude from auto generation
        if let Some(excluded_routes) = excluded_routes {
            routes.retain(|p| !excluded_routes.iter().any(|e| e == p.path()))
        }
        routes
    }
}

#[tracing::instrument(level = "trace", fields(error), skip_all)]
async fn render_app_async_helper(
    options: &LeptosOptions,
    app: impl FnOnce() -> View + 'static,
    mut res_options: ResponseOptions,
    additional_context: impl Fn() + 'static + Clone + Send,
) -> Result<worker::Response, worker::Error> {
    let (stream, runtime) =
        leptos::ssr::render_to_stream_in_order_with_prefix_undisposed_with_context(
            app,
            move || "".into(),
            additional_context,
        );

    let html = build_async_response(stream, options, runtime).await;

    let status = res_options.status.unwrap_or(200);

    let mut res = worker::Response::from_html(html)?;

    res.headers_mut().set("Content-Type", "text/html")?;

    // Add headers manipulated in the response
    for (key, value) in res_options.headers.into_iter() {
        res_options.append_header(&key, &value)?;
    }

    Ok(res.with_status(status))
}

#[tracing::instrument(level = "trace", fields(error), skip_all)]
pub fn render_app_async_with_context<'a, 'b, IV, AppFn>(
    method: LeptosMethod,
    path: &'a str,
    cf_router: worker::Router<'b, WorkerRouterData<IV, AppFn>>,
) -> worker::Router<'b, WorkerRouterData<IV, AppFn>>
where
    IV: IntoView + 'static,
    AppFn: Fn() -> IV + Clone + Send + 'static,
{
    let handler = |mut req: worker::Request,
                   ctx: worker::RouteContext<WorkerRouterData<IV, AppFn>>| async move {
        let options = ctx.data.options;
        let app_fn = ctx.data.app_fn;
        let res_options = ResponseOptions::default();
        let app = {
            let app_fn = app_fn.clone();
            let res_options = res_options.clone();

            let request_parts = generate_request_parts(&mut req).await?;

            move || {
                provide_contexts(request_parts.url.to_string(), request_parts, res_options);
                (app_fn)().into_view()
            }
        };

        render_app_async_helper(&options, app, res_options, || {}).await
    };

    match method {
        LeptosMethod::Get => cf_router.get_async(path, handler),
        LeptosMethod::Post => cf_router.post_async(path, handler),
        LeptosMethod::Put => cf_router.put_async(path, handler),
        LeptosMethod::Delete => cf_router.delete_async(path, handler),
        LeptosMethod::Patch => cf_router.patch_async(path, handler),
    }
}

#[tracing::instrument(level = "trace", fields(error), skip_all)]
pub fn render_app_to_stream_in_order_with_context<'a, 'b, IV, AppFn>(
    method: LeptosMethod,
    path: &'a str,
    cf_router: worker::Router<'b, WorkerRouterData<IV, AppFn>>,
) -> worker::Router<'b, WorkerRouterData<IV, AppFn>>
where
    IV: IntoView + 'static,
    AppFn: Fn() -> IV + Clone + Send + 'static,
{
    let handler = |mut req: worker::Request,
                   ctx: worker::RouteContext<WorkerRouterData<IV, AppFn>>| async move {
        let options = ctx.data.options;
        let app_fn = ctx.data.app_fn;
        let res_options = ResponseOptions::default();
        let app = {
            let app_fn = app_fn.clone();
            let res_options = res_options.clone();

            let request_parts = generate_request_parts(&mut req).await?;

            move || {
                provide_contexts(request_parts.url.to_string(), request_parts, res_options);
                (app_fn)().into_view()
            }
        };

        stream_app_in_order(&options, app, res_options, || {}).await
    };

    match method {
        LeptosMethod::Get => cf_router.get_async(path, handler),
        LeptosMethod::Post => cf_router.post_async(path, handler),
        LeptosMethod::Put => cf_router.put_async(path, handler),
        LeptosMethod::Delete => cf_router.delete_async(path, handler),
        LeptosMethod::Patch => cf_router.patch_async(path, handler),
    }
}

async fn stream_app_in_order(
    options: &LeptosOptions,
    app: impl FnOnce() -> View + 'static,
    res_options: ResponseOptions,
    additional_context: impl Fn() + 'static + Clone + Send,
) -> worker::Result<worker::Response> {
    let (stream, runtime) =
        leptos::ssr::render_to_stream_in_order_with_prefix_undisposed_with_context(
            app,
            move || generate_head_metadata_separated().1.into(),
            additional_context,
        );

    build_stream_response(options, res_options, stream, runtime).await
}

#[tracing::instrument(level = "trace", fields(error), skip_all)]
async fn build_stream_response(
    options: &LeptosOptions,
    mut res_options: ResponseOptions,
    stream: impl Stream<Item = String> + 'static,
    runtime: RuntimeId,
) -> worker::Result<worker::Response> {
    let mut stream = Box::pin(stream);

    // wait for any blocking resources to load before pulling metadata
    let first_app_chunk = stream.next().await.unwrap_or_default();

    let (head, tail) = html_parts_separated(options, use_context::<MetaContext>().as_ref());

    let mut stream = Box::pin(
        futures::stream::once(async move { head.clone() })
            .chain(futures::stream::once(async move { first_app_chunk }).chain(stream))
            .chain(futures::stream::once(async move {
                runtime.dispose();
                tail.to_string()
            }))
            .map(|html| worker::Result::Ok(html.into_bytes())),
    );

    // Get the first and second in the stream, which renders the app shell, and thus allows Resources to run
    let first_chunk = stream.next().await;
    let second_chunk = stream.next().await;

    let status = res_options.status.unwrap_or(200);

    let complete_stream =
        futures::stream::iter([first_chunk.unwrap(), second_chunk.unwrap()]).chain(stream);
    let mut response = worker::Response::from_stream(complete_stream)?;
    response.headers_mut().set("Content-Type", "text/html")?;

    // Add headers manipulated in the response
    for (key, value) in res_options.headers.into_iter() {
        res_options.append_header(&key, &value)?;
    }

    Ok(response.with_status(status))
}

#[tracing::instrument(level = "trace", fields(error), skip_all)]
async fn stream_app(
    options: &LeptosOptions,
    app: impl FnOnce() -> View + 'static,
    res_options: ResponseOptions,
    additional_context: impl Fn() + 'static + Clone,
    replace_blocks: bool,
) -> worker::Result<worker::Response> {
    let (stream, runtime) =
        render_to_stream_with_prefix_undisposed_with_context_and_block_replacement(
            app,
            move || generate_head_metadata_separated().1.into(),
            additional_context,
            replace_blocks,
        );

    build_stream_response(options, res_options, stream, runtime).await
}

fn provide_contexts(path: String, req: RequestParts, default_res_options: ResponseOptions) {
    let integration = ServerIntegration { path };
    provide_context(RouterIntegrationContext::new(integration));
    provide_context(MetaContext::new());
    provide_context(req);
    provide_context(default_res_options);
    provide_server_redirect(move |path| redirect(path));
    #[cfg(feature = "nonce")]
    leptos::nonce::provide_nonce(cx);
}

impl ResponseOptions {
    /// Insert a header, overwriting any previous value with the same key
    pub fn insert_header(&mut self, key: &str, value: &str) -> worker::Result<()> {
        self.headers.set(key, value)
    }
    /// Append a header, leaving any header with the same key intact
    pub fn append_header(&mut self, key: &str, value: &str) -> worker::Result<()> {
        self.headers.append(key, value)
    }
}

impl<'a, IV, AppFn> LeptosRoutes for worker::Router<'a, WorkerRouterData<IV, AppFn>>
where
    IV: IntoView + 'static,
    AppFn: Fn() -> IV + Clone + Send + 'static,
{
    fn leptos_routes(self, paths: Vec<RouteListing>) -> Self {
        let mut cf_router = self;
        for listing in paths.iter() {
            let path = listing.path();
            let mode = listing.mode();
            for method in listing.methods() {
                cf_router = match mode {
                    SsrMode::OutOfOrder => {
                        render_app_to_stream_with_context(method, path, cf_router)
                    }
                    SsrMode::PartiallyBlocked => {
                        render_app_to_stream_with_context_and_replace_blocks(
                            method, path, cf_router,
                        )
                    }
                    SsrMode::Async => render_app_async_with_context(method, path, cf_router),
                    SsrMode::InOrder => {
                        render_app_to_stream_in_order_with_context(method, path, cf_router)
                    }
                }
            }
        }
        cf_router
    }
}

impl Default for ResponseOptions {
    fn default() -> Self {
        Self {
            status: Some(200),
            headers: Headers::new(),
        }
    }
}
