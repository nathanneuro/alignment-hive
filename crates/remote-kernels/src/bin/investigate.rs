//! Investigation binary for debugging pod creation and Jupyter readiness.
//!
//! Usage: cargo run --bin investigate -- [command]

use std::collections::HashMap;

use remote_kernels::config::Config;
use remote_kernels::runpod::client::RunPodClient;
use remote_kernels::runpod::types::PodCreateInput;

fn project_dir() -> &'static std::path::Path {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
}

fn load_env() -> String {
    let dir = project_dir();
    let _ = dotenvy::from_path(dir.join(".env.local"));
    let _ = dotenvy::from_path(dir.join(".env"));
    std::env::var("RUNPOD_API_KEY").expect("RUNPOD_API_KEY not set")
}

fn build_create_input() -> PodCreateInput {
    let config = Config::load(project_dir()).expect("Failed to load config");

    let ssh_pubkey = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIFakeKeyForTesting test@test";
    let jupyter_token = "deadbeef1234567890abcdef1234567890abcdef1234567890abcdef12345678";

    let mut env = HashMap::new();
    env.insert("PUBLIC_KEY".to_string(), ssh_pubkey.to_string());
    env.insert("JUPYTER_PASSWORD".to_string(), jupyter_token.to_string());

    PodCreateInput {
        name: "investigate-test".to_string(),
        image_name: config.image_name.clone(),
        gpu_type_ids: config.gpu_type_ids.clone(),
        gpu_count: Some(config.runpod.gpu_count),
        cloud_type: Some(config.runpod.cloud_type.clone()),
        container_disk_in_gb: Some(config.runpod.container_disk_gb),
        volume_in_gb: if config.runpod.volume_gb > 0 {
            Some(config.runpod.volume_gb)
        } else {
            None
        },
        volume_mount_path: Some(config.runpod.volume_mount_path.clone()),
        network_volume_id: config.runpod.network_volume_id.clone(),
        ports: Some(vec!["8888/http".to_string(), "22/tcp".to_string()]),
        env: Some(env),
        // NOTE: dockerStartCmd is NOT used — it replaces the container's CMD
        // which prevents RunPod images from starting services (Jupyter, SSH).
        docker_start_cmd: None,
        extra: HashMap::new(),
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter("info")
        .with_writer(std::io::stderr)
        .init();

    let args: Vec<String> = std::env::args().collect();
    let cmd = args.get(1).map(String::as_str).unwrap_or("help");

    match cmd {
        "serialize" => {
            let input = build_create_input();
            println!("{}", serde_json::to_string_pretty(&input)?);
        }
        "create" | "create-bare" | "create-sleep" => {
            let api_key = load_env();
            let client = RunPodClient::new(api_key);
            let mut input = build_create_input();

            if cmd == "create-bare" {
                println!("Creating pod WITHOUT dockerStartCmd...");
                input.docker_start_cmd = None;
            } else if cmd == "create-sleep" {
                println!("Creating pod WITH bash -c wrapper + sleep infinity...");
                input.docker_start_cmd = Some(vec![
                    "bash".to_string(),
                    "-c".to_string(),
                    "apt-get update -qq && apt-get install -y -qq rsync; sleep infinity"
                        .to_string(),
                ]);
            } else {
                println!("Creating pod WITH dockerStartCmd...");
            }
            println!("Request:\n{}", serde_json::to_string_pretty(&input)?);

            let pod = client.create_pod(&input).await?;
            println!("\nPod created: {}", pod.id);
            println!("Status: {:?}", pod.desired_status);
            println!("GPU: {}", pod.gpu_display_name());

            println!("\nWaiting for RUNNING...");
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                let p = client.get_pod(&pod.id).await?;
                println!("  Status: {:?}", p.desired_status);
                if p.is_running() {
                    println!("Pod is RUNNING!");
                    break;
                }
            }
            println!("\nPod ID: {}", pod.id);
            println!(
                "Don't forget to terminate: cargo run --bin investigate -- cleanup {}",
                pod.id
            );
        }
        "urls" => {
            // Try various URL patterns against a running pod
            let pod_id = args.get(2).expect("Usage: investigate urls <pod_id>");
            let client = reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .build()?;

            let patterns = [
                format!("https://{pod_id}-8888.proxy.runpod.net/api"),
                format!("https://{pod_id}-8888.proxy.runpod.net/"),
                format!("https://{pod_id}-8888.proxy.runpod.ai/api"),
                format!("https://{pod_id}-8888.proxy.runpod.ai/"),
                format!("https://{pod_id}.proxy.runpod.net/api"),
                format!("https://{pod_id}.proxy.runpod.net/"),
                // RunPod serverless proxy pattern
                format!("https://proxy.runpod.net/v1/{pod_id}/8888/api"),
                format!("https://proxy.runpod.net/v1/{pod_id}/8888/"),
            ];

            println!("Testing URL patterns for pod {pod_id}:\n");
            for url in &patterns {
                match client
                    .get(url)
                    .timeout(std::time::Duration::from_secs(5))
                    .send()
                    .await
                {
                    Ok(resp) => {
                        let status = resp.status();
                        let location = resp
                            .headers()
                            .get("location")
                            .map(|v| v.to_str().unwrap_or("?").to_string());
                        let body_preview = resp.text().await.unwrap_or_default();
                        let body_preview = &body_preview[..body_preview.len().min(150)];
                        println!("  {status}  {url}");
                        if let Some(loc) = location {
                            println!("         -> Redirect: {loc}");
                        }
                        if !body_preview.is_empty() {
                            println!("         Body: {body_preview}");
                        }
                    }
                    Err(e) => {
                        println!("  ERR    {url}");
                        println!("         {e}");
                    }
                }
            }
        }
        "graphql" => {
            // Query GraphQL for pod runtime info (raw, no deserialization)
            let pod_id = args.get(2).expect("Usage: investigate graphql <pod_id>");
            let api_key = load_env();
            let http_client = reqwest::Client::new();
            let query = serde_json::json!({
                "query": format!(
                    r#"query {{ pod(input: {{podId: "{pod_id}"}}) {{
                        id
                        name
                        desiredStatus
                        imageName
                        env
                        dockerArgs
                        runtime {{
                            uptimeInSeconds
                            ports {{ ip isIpPublic privatePort publicPort type }}
                            gpus {{ id gpuUtilPercent memoryUtilPercent }}
                            container {{
                                cpuPercent
                                memoryPercent
                            }}
                        }}
                        lastStatusChange
                    }} }}"#
                )
            });
            let resp = http_client
                .post("https://api.runpod.io/graphql")
                .bearer_auth(&api_key)
                .json(&query)
                .send()
                .await?;
            let body = resp.text().await?;
            println!("\nFull GraphQL response:\n{body}");
        }
        "probe" => {
            let pod_id = args
                .get(2)
                .expect("Usage: investigate probe <pod_id> [token]");
            let token = args
                .get(3)
                .map(String::as_str)
                .unwrap_or("deadbeef1234567890abcdef1234567890abcdef1234567890abcdef12345678");
            let url = args.get(4).map(String::as_str);
            probe_jupyter(pod_id, token, url).await;
        }
        "cleanup" => {
            let pod_id = args.get(2).expect("Usage: investigate cleanup <pod_id>");
            let api_key = load_env();
            let client = RunPodClient::new(api_key);
            println!("Terminating pod {pod_id}...");
            client.terminate_pod(pod_id).await?;
            println!("Done.");
        }
        "reconnect-test" => {
            // Full reconnection test:
            // 1. Create a bare pod
            // 2. Write state.json simulating a saved session
            // 3. Stop the pod
            // 4. Verify state.json has reconnection details
            // 5. Try to restart and reconnect
            let api_key = load_env();
            let client = RunPodClient::new(api_key);
            let mut input = build_create_input();
            input.docker_start_cmd = None;

            println!("Step 1: Creating pod...");
            let pod = client.create_pod(&input).await?;
            println!("  Pod: {}", pod.id);

            println!("\nStep 2: Waiting for RUNNING...");
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                let p = client.get_pod(&pod.id).await?;
                if p.is_running() {
                    println!("  RUNNING!");
                    break;
                }
            }

            // Wait for Jupyter to be reachable
            println!("\nStep 3: Waiting for Jupyter...");
            let token = "deadbeef1234567890abcdef1234567890abcdef1234567890abcdef12345678";
            let jupyter = remote_kernels::jupyter::rest::JupyterClient::new(&pod.id, token);
            jupyter.wait_until_ready().await?;
            println!("  Jupyter ready!");

            println!("\nStep 4: Stopping pod...");
            client.stop_pod(&pod.id).await?;
            println!("  Stopped!");

            // Wait for it to fully stop
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            let stopped = client.get_pod(&pod.id).await?;
            println!(
                "  Status: {:?}",
                stopped.desired_status.as_deref().unwrap_or("?")
            );

            println!("\nStep 5: Restarting pod...");
            let restarted = client.resume_pod(&pod.id).await?;
            println!("  Restart response status: {:?}", restarted.desired_status);

            println!("\nStep 6: Waiting for RUNNING again...");
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                let p = client.get_pod(&pod.id).await?;
                let status = p.desired_status.as_deref().unwrap_or("?");
                println!("  Status: {status}");
                if p.is_running() {
                    println!("  RUNNING!");
                    break;
                }
            }

            println!("\nStep 7: Probing Jupyter with SAME token...");
            tokio::time::sleep(std::time::Duration::from_secs(10)).await;
            let jupyter2 = remote_kernels::jupyter::rest::JupyterClient::new(&pod.id, token);
            match jupyter2.wait_until_ready().await {
                Ok(()) => println!("  Jupyter ready with original token! Reconnection works!"),
                Err(e) => println!("  FAILED: {e}"),
            }

            println!("\nStep 8: Cleaning up...");
            client.terminate_pod(&pod.id).await?;
            println!("  Terminated.");
            println!("\nTest complete!");
        }
        "kernel-test" => {
            // Create a kernel and run code on it
            let pod_id = args
                .get(2)
                .expect("Usage: investigate kernel-test <pod_id> [token]");
            let token = args
                .get(3)
                .map(String::as_str)
                .unwrap_or("deadbeef1234567890abcdef1234567890abcdef1234567890abcdef12345678");

            let jupyter = remote_kernels::jupyter::rest::JupyterClient::new(pod_id, token);

            println!("Creating kernel...");
            let kernel = jupyter.create_kernel().await?;
            println!("Kernel created: {} ({})", kernel.id, kernel.name);

            println!("Listing kernels...");
            let kernels = jupyter.list_kernels().await?;
            println!("Kernels: {kernels:#?}");

            println!("\nKernel ID: {}", kernel.id);
            println!("Kernel test passed!");
        }
        _ => {
            println!("Commands:");
            println!("  serialize           — Print the JSON request body");
            println!("  create              — Create a pod");
            println!("  urls <pod_id>       — Try various proxy URL patterns");
            println!("  graphql <pod_id>    — Query GraphQL for pod runtime info");
            println!("  probe <id> [token] [base_url]  — Probe Jupyter readiness");
            println!("  kernel-test <id> [token]  — Create a kernel and test it");
            println!("  cleanup <id>        — Terminate a pod");
        }
    }

    Ok(())
}

async fn probe_jupyter(pod_id: &str, token: &str, base_url_override: Option<&str>) {
    let client = reqwest::Client::new();
    let base_url = base_url_override
        .map(String::from)
        .unwrap_or_else(|| format!("https://{pod_id}-8888.proxy.runpod.net"));

    println!("Probing: {base_url}/api");
    println!("Token: {token}\n");

    for attempt in 1..=20 {
        // Try with auth
        let url = format!("{base_url}/api");
        match client
            .get(&url)
            .header("Authorization", format!("token {token}"))
            .timeout(std::time::Duration::from_secs(5))
            .send()
            .await
        {
            Ok(resp) => {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                if status.is_success() {
                    println!("Attempt {attempt}: {status} — SUCCESS!");
                    println!("Body: {}", &body[..body.len().min(200)]);
                    return;
                }
                println!(
                    "Attempt {attempt}: {status} — {}",
                    &body[..body.len().min(200)]
                );
            }
            Err(e) => {
                println!("Attempt {attempt}: ERROR — {e}");
            }
        }
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
    }
    println!("\nGave up.");
}
