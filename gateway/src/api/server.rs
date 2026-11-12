use super::*;

pub(crate) async fn bind_gateway(host: &str, port: u16) -> anyhow::Result<tokio::net::TcpListener> {
    let addr: SocketAddr = format!("{host}:{port}").parse()?;
    Ok(tokio::net::TcpListener::bind(addr).await?)
}

/// The running process is the installed background service, so Settings can restart it.
pub(crate) fn service_can_restart(home: &std::path::Path) -> bool {
    if std::env::var("HBOTS_SERVICE").ok().as_deref() != Some("1") {
        return false;
    }
    let os = std::env::consts::OS;
    service::supported(os) && service::installed(&cli::service_spec(home), os)
}

/// Ctrl-C or SIGTERM: the gateway stops its tunnel before it exits.
pub(crate) async fn shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        let mut term = signal(SignalKind::terminate()).expect("SIGTERM handler");
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = term.recv() => {}
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}
