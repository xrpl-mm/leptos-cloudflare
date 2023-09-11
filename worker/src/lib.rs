use app::*;
use futures::StreamExt;
use leptos::*;
use leptos_dom::ssr::render_to_stream;
use worker::*;

mod utils;

#[event(fetch)]
pub async fn main(req: Request, env: worker::Env, _ctx: worker::Context) -> Result<Response> {
    log_request(&req);

    utils::set_panic_hook();

    let router = Router::new();

    router.get("/", |_req: Request, _ctx| {
        let pkg_path = "/site/client";
        let head = format!(
            r#"<!DOCTYPE html>
            <html lang="en">
                <head>
                    <meta charset="utf-8"/>
                    <meta name="viewport" content="width=device-width, initial-scale=1"/>
                    <link rel="modulepreload" href="{pkg_path}.js">
                    <link rel="preload" href="{pkg_path}_bg.wasm" as="fetch" type="application/wasm" crossorigin="">
                    <script type="module">import init, {{ hydrate }} from '{pkg_path}.js'; init('{pkg_path}_bg.wasm').then(hydrate);</script>
                </head>
                <body>"#
        );

        let tail = "</body></html>";

        let stream = futures::stream::once(async move { head.clone() })
            .chain(render_to_stream(|cx| view! { cx,  <App /> }.into_view(cx)))
            .chain(futures::stream::once(async { tail.to_string() }))
            .inspect(|html| println!("{html}"))
            .map(|html| Result::Ok(html.into_bytes()));
        let mut response = Response::from_stream(stream)?;
        response
            .headers_mut()
            .set("Content-Type", "text/html")?;
        Ok(response)
    }).get_async("/site/:resource", |_req, ctx| async move {
        let resource = match ctx.param("resource") {
            Some(resource) => resource,
            None => {
                return Response::error("Not found", 404)
            }
        };

        let store = ctx.env.kv("__STATIC_CONTENT")?;
        let file_path = match ctx.env.asset_key(resource) {
            Ok(file_path) => file_path,
            Err(_) => {
                return Response::error("Not found", 404)
            }
        };

        if let Some(bytes) = store.get(&file_path).bytes().await? {
            let mut response = Response::from_bytes(bytes)?;
            let content_type = match mime_guess::from_path(file_path).first() {
                Some(content_type) => content_type,
                None => {
                    return Response::error("Unsupported file type", 415)
                },
            };
            response
                .headers_mut()
                .set("Content-Type", content_type.essence_str())?;
            Ok(response)
        } else {
            Response::error("Not found", 404)
        }
    }).run(req, env).await
}

fn log_request(req: &Request) {
    console_log!(
        "{} - [{}], located at: {:?}, within: {}",
        Date::now().to_string(),
        req.path(),
        req.cf().coordinates().unwrap_or_default(),
        req.cf().region().unwrap_or_else(|| "unknown region".into())
    );
}
