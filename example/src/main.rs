mod app;
mod utils;

#[cfg(feature = "ssr")]
#[worker::event(fetch)]
pub async fn main(
    req: worker::Request,
    env: worker::Env,
    _ctx: worker::Context,
) -> worker::Result<worker::Response> {
    use std::{net::SocketAddr, str::FromStr};

    use app::App;
    use leptos::*;
    use leptos_cloudflare::{self, LeptosRoutes};
    use utils::set_panic_hook;
    use worker::Router;

    // Automatic registration of server functions doesn't work for wasm32 server
    utils::handle_register_server_fn(app::GetPost::register_explicit());
    utils::handle_register_server_fn(app::ListPostMetadata::register_explicit());

    set_panic_hook();

    let routes = leptos_cloudflare::generate_route_list(|| view! { <App /> }.into_view());

    // Manually specify options, because worker doesn't have access to local fs
    let leptos_options = LeptosOptions::builder()
        .output_name(String::from("example"))
        .site_root(String::from("target/site"))
        .site_pkg_dir(String::from("pkg"))
        .env(leptos_config::Env::DEV)
        .site_addr(SocketAddr::from_str("127.0.0.1:3000").unwrap())
        .reload_port(3001)
        .build();

    let router = Router::with_data(leptos_cloudflare::WorkerRouterData {
        options: leptos_options.clone(),
        app_fn: app::App,
    });

    worker::console_debug!("Routes: {:?}", routes);

    router
        .leptos_routes(routes)
        .post_async("/api/:fn_name", leptos_cloudflare::handle_server_fns)
        .run(req, env)
        .await
}

#[cfg(feature = "ssr")]
fn log_request(req: &worker::Request) {
    worker::console_log!(
        "{} - [{}], located at: {:?}, within: {}",
        worker::Date::now().to_string(),
        req.path(),
        req.cf().coordinates().unwrap_or_default(),
        req.cf().region().unwrap_or_else(|| "unknown region".into())
    );
}

fn main() {}
