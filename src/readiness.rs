use crate::config::{ReadyConfig, ReadyProbe, TaskConfig};
use anyhow::{Context, Result, bail};
use reqwest::Url;
use std::net::SocketAddr;
use std::process::Stdio;
use tokio::net::TcpStream;
use tokio::process::Command;
use tokio::time::{Instant, sleep, timeout};

pub async fn check_tcp_once(target: &str) -> Result<()> {
    let (host, port) = parse_host_port(target)?;
    TcpStream::connect((host.as_str(), port))
        .await
        .with_context(|| format!("tcp readiness failed for {target}"))?;
    Ok(())
}

pub async fn check_http_once(url: &str) -> Result<()> {
    let response = reqwest::get(url)
        .await
        .with_context(|| format!("http readiness failed for {url}"))?;
    if response.status().is_success() || response.status().is_redirection() {
        Ok(())
    } else {
        bail!("http readiness for {url} returned {}", response.status())
    }
}

pub fn parse_host_port(input: &str) -> Result<(String, u16)> {
    if input.starts_with("http://") || input.starts_with("https://") {
        let url = Url::parse(input)?;
        let host = url
            .host_str()
            .context("url must include a host")?
            .to_string();
        let port = url
            .port_or_known_default()
            .context("url must include a port or known scheme")?;
        return Ok((host, port));
    }

    if let Ok(addr) = input.parse::<SocketAddr>() {
        return Ok((addr.ip().to_string(), addr.port()));
    }

    let (host, port) = input
        .rsplit_once(':')
        .with_context(|| format!("expected host:port target, got '{input}'"))?;
    Ok((host.to_string(), port.parse()?))
}

pub fn declared_ports(task: &TaskConfig) -> Vec<(String, u16)> {
    task.ready
        .probes
        .iter()
        .filter_map(|probe| match probe {
            ReadyProbe::Tcp(target) | ReadyProbe::Http(target) => parse_host_port(target).ok(),
            ReadyProbe::Command(_) | ReadyProbe::LogContains(_) => None,
        })
        .collect()
}

pub async fn port_is_open(host: &str, port: u16) -> bool {
    timeout(
        std::time::Duration::from_millis(200),
        TcpStream::connect((host, port)),
    )
    .await
    .is_ok_and(|result| result.is_ok())
}

pub async fn wait_for_ready(
    task: &TaskConfig,
    log_snapshot: impl Fn() -> Vec<String>,
) -> Result<()> {
    if task.ready.probes.is_empty() {
        return Ok(());
    }

    let deadline = Instant::now() + task.ready.timeout;
    loop {
        let mut all_ready = true;
        for probe in &task.ready.probes {
            if !probe_ready(probe, task, &log_snapshot).await {
                all_ready = false;
                break;
            }
        }
        if all_ready {
            return Ok(());
        }
        if Instant::now() >= deadline {
            bail!(
                "readiness timed out after {}s",
                task.ready.timeout.as_secs()
            );
        }
        sleep(task.ready.interval).await;
    }
}

async fn probe_ready(
    probe: &ReadyProbe,
    task: &TaskConfig,
    log_snapshot: &impl Fn() -> Vec<String>,
) -> bool {
    match probe {
        ReadyProbe::Tcp(target) => check_tcp_once(target).await.is_ok(),
        ReadyProbe::Http(url) => check_http_once(url).await.is_ok(),
        ReadyProbe::Command(command) => check_command_once(command, task).await.is_ok(),
        ReadyProbe::LogContains(needle) => log_snapshot().iter().any(|line| line.contains(needle)),
    }
}

async fn check_command_once(command: &str, task: &TaskConfig) -> Result<()> {
    let parts = shell_words::split(command)?;
    let (program, args) = parts
        .split_first()
        .context("ready.command cannot be empty")?;
    let mut cmd = Command::new(program);
    cmd.args(args).stdout(Stdio::null()).stderr(Stdio::null());
    if let Some(cwd) = &task.cwd {
        cmd.current_dir(cwd);
    }
    cmd.envs(&task.env);
    let status = cmd.status().await?;
    if status.success() {
        Ok(())
    } else {
        bail!("ready.command exited with {status}")
    }
}

#[allow(dead_code)]
fn _assert_ready_config_send_sync(_: ReadyConfig) {}
