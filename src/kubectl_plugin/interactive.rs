//! Interactive mode for kubectl-stellar plugin
//!
//! Provides a menu-driven interface with guided workflows for common operations

use anyhow::Result;
use colored::*;
use dialoguer::{theme::ColorfulTheme, Confirm, Input, Select};
use std::process::Command;

/// Main interactive mode entry point
pub async fn run_interactive_mode() -> Result<()> {
    print_banner();
    
    loop {
        let options = vec![
            "Deploy a new Stellar node",
            "View node status",
            "Troubleshoot a node",
            "Scale Horizon deployment",
            "Backup and restore",
            "View logs",
            "Network diagnostics",
            "Exit",
        ];
        
        let selection = Select::with_theme(&ColorfulTheme::default())
            .with_prompt("What would you like to do?")
            .items(&options)
            .default(0)
            .interact()?;
        
        match selection {
            0 => deploy_node_wizard().await?,
            1 => view_status_wizard().await?,
            2 => troubleshoot_wizard().await?,
            3 => scale_horizon_wizard().await?,
            4 => backup_restore_wizard().await?,
            5 => view_logs_wizard().await?,
            6 => network_diagnostics_wizard().await?,
            7 => {
                println!("{}", "Goodbye! 👋".bright_cyan());
                break;
            }
            _ => unreachable!(),
        }
        
        println!();
    }
    
    Ok(())
}

fn print_banner() {
    println!("{}", "╔═══════════════════════════════════════════════════════════╗".bright_cyan());
    println!("{}", "║  ✦ Stellar-K8s Interactive Mode                          ║".bright_cyan());
    println!("{}", "║  Cloud-Native Stellar Infrastructure on Kubernetes       ║".bright_magenta());
    println!("{}", "║  Built with Rust 🦀 · Powered by kube-rs                 ║".bright_black());
    println!("{}", "╚═══════════════════════════════════════════════════════════╝".bright_cyan());
    println!();
}

/// Guided workflow for deploying a new Stellar node
async fn deploy_node_wizard() -> Result<()> {
    println!("{}", "\n🚀 Deploy New Stellar Node".bright_green().bold());
    println!("{}", "This wizard will guide you through deploying a new node.\n".bright_black());
    
    // Step 1: Node type
    let node_types = vec!["Validator", "Horizon", "Soroban RPC"];
    let node_type_idx = Select::with_theme(&ColorfulTheme::default())
        .with_prompt("Select node type")
        .items(&node_types)
        .default(0)
        .interact()?;
    let node_type = node_types[node_type_idx].to_lowercase();
    
    // Step 2: Node name
    let node_name: String = Input::with_theme(&ColorfulTheme::default())
        .with_prompt("Enter node name")
        .default(format!("my-{}", node_type))
        .interact_text()?;
    
    // Step 3: Namespace
    let namespace: String = Input::with_theme(&ColorfulTheme::default())
        .with_prompt("Enter namespace")
        .default("default".to_string())
        .interact_text()?;
    
    // Step 4: Network
    let networks = vec!["mainnet", "testnet", "futurenet"];
    let network_idx = Select::with_theme(&ColorfulTheme::default())
        .with_prompt("Select Stellar network")
        .items(&networks)
        .default(1) // testnet for safety
        .interact()?;
    let network = networks[network_idx];
    
    // Step 5: Storage size
    let storage_size: String = Input::with_theme(&ColorfulTheme::default())
        .with_prompt("Enter storage size (e.g., 100Gi)")
        .default("100Gi".to_string())
        .validate_with(|input: &String| -> Result<(), &str> {
            if input.ends_with("Gi") || input.ends_with("Ti") {
                Ok(())
            } else {
                Err("Storage size must end with Gi or Ti")
            }
        })
        .interact_text()?;
    
    // Step 6: Generate manifest
    println!("\n{}", "📝 Generating manifest...".bright_yellow());
    
    let manifest = generate_node_manifest(&node_name, &node_type, &namespace, network, &storage_size);
    
    println!("\n{}", "Generated manifest:".bright_cyan());
    println!("{}", "─".repeat(60).bright_black());
    println!("{}", manifest);
    println!("{}", "─".repeat(60).bright_black());
    
    // Step 7: Confirm deployment
    let confirm = Confirm::with_theme(&ColorfulTheme::default())
        .with_prompt("Deploy this node?")
        .default(false)
        .interact()?;
    
    if confirm {
        println!("\n{}", "🚀 Deploying node...".bright_green());
        
        // Write manifest to temp file and apply
        let temp_file = format!("/tmp/{}-stellarnode.yaml", node_name);
        std::fs::write(&temp_file, manifest)?;
        
        let output = Command::new("kubectl")
            .args(&["apply", "-f", &temp_file])
            .output()?;
        
        if output.status.success() {
            println!("{}", "✓ Node deployed successfully!".bright_green());
            println!("\n{}", "Next steps:".bright_cyan());
            println!("  • Check status: {}", format!("kubectl stellar status {}", node_name).bright_yellow());
            println!("  • View logs: {}", format!("kubectl stellar logs {}", node_name).bright_yellow());
            println!("  • Monitor: {}", "kubectl stellar status --watch".bright_yellow());
        } else {
            println!("{}", "✗ Deployment failed:".bright_red());
            println!("{}", String::from_utf8_lossy(&output.stderr));
        }
        
        // Cleanup
        std::fs::remove_file(&temp_file).ok();
    } else {
        println!("{}", "Deployment cancelled.".bright_yellow());
    }
    
    Ok(())
}

/// View node status with filtering options
async fn view_status_wizard() -> Result<()> {
    println!("{}", "\n📊 View Node Status".bright_green().bold());
    
    let options = vec![
        "All nodes in current namespace",
        "All nodes across all namespaces",
        "Specific node",
        "Nodes by type (validator/horizon/soroban)",
    ];
    
    let selection = Select::with_theme(&ColorfulTheme::default())
        .with_prompt("What would you like to view?")
        .items(&options)
        .default(0)
        .interact()?;
    
    match selection {
        0 => {
            println!("\n{}", "Fetching nodes in current namespace...".bright_yellow());
            let output = Command::new("kubectl")
                .args(&["stellar", "status"])
                .output()?;
            println!("{}", String::from_utf8_lossy(&output.stdout));
        }
        1 => {
            println!("\n{}", "Fetching nodes across all namespaces...".bright_yellow());
            let output = Command::new("kubectl")
                .args(&["stellar", "status", "--all-namespaces"])
                .output()?;
            println!("{}", String::from_utf8_lossy(&output.stdout));
        }
        2 => {
            let node_name: String = Input::with_theme(&ColorfulTheme::default())
                .with_prompt("Enter node name")
                .interact_text()?;
            
            println!("\n{}", format!("Fetching status for {}...", node_name).bright_yellow());
            let output = Command::new("kubectl")
                .args(&["stellar", "status", &node_name])
                .output()?;
            println!("{}", String::from_utf8_lossy(&output.stdout));
        }
        3 => {
            let types = vec!["validator", "horizon", "soroban-rpc"];
            let type_idx = Select::with_theme(&ColorfulTheme::default())
                .with_prompt("Select node type")
                .items(&types)
                .interact()?;
            
            println!("\n{}", format!("Fetching {} nodes...", types[type_idx]).bright_yellow());
            let output = Command::new("kubectl")
                .args(&["get", "stellarnodes", "--all-namespaces", "-l", &format!("node-type={}", types[type_idx])])
                .output()?;
            println!("{}", String::from_utf8_lossy(&output.stdout));
        }
        _ => unreachable!(),
    }
    
    Ok(())
}

/// Interactive troubleshooting wizard
async fn troubleshoot_wizard() -> Result<()> {
    println!("{}", "\n🔧 Troubleshooting Wizard".bright_green().bold());
    println!("{}", "This wizard will help diagnose common issues.\n".bright_black());
    
    let node_name: String = Input::with_theme(&ColorfulTheme::default())
        .with_prompt("Enter node name to troubleshoot")
        .interact_text()?;
    
    println!("\n{}", "Running diagnostics...".bright_yellow());
    
    // Check 1: Node exists
    println!("\n{}", "1. Checking if node exists...".bright_cyan());
    let output = Command::new("kubectl")
        .args(&["get", "stellarnode", &node_name])
        .output()?;
    
    if !output.status.success() {
        println!("{}", "✗ Node not found in current namespace".bright_red());
        
        let check_all = Confirm::with_theme(&ColorfulTheme::default())
            .with_prompt("Search in all namespaces?")
            .default(true)
            .interact()?;
        
        if check_all {
            let output = Command::new("kubectl")
                .args(&["get", "stellarnode", &node_name, "--all-namespaces"])
                .output()?;
            println!("{}", String::from_utf8_lossy(&output.stdout));
        }
        return Ok(());
    }
    println!("{}", "✓ Node exists".bright_green());
    
    // Check 2: Pod status
    println!("\n{}", "2. Checking pod status...".bright_cyan());
    let output = Command::new("kubectl")
        .args(&["get", "pods", "-l", &format!("app.kubernetes.io/instance={}", node_name)])
        .output()?;
    println!("{}", String::from_utf8_lossy(&output.stdout));
    
    // Check 3: Recent events
    println!("\n{}", "3. Checking recent events...".bright_cyan());
    let output = Command::new("kubectl")
        .args(&["get", "events", "--sort-by=.lastTimestamp", "--field-selector", &format!("involvedObject.name={}", node_name)])
        .output()?;
    println!("{}", String::from_utf8_lossy(&output.stdout));
    
    // Check 4: PVC status
    println!("\n{}", "4. Checking PVC status...".bright_cyan());
    let output = Command::new("kubectl")
        .args(&["get", "pvc", "-l", &format!("app.kubernetes.io/instance={}", node_name)])
        .output()?;
    println!("{}", String::from_utf8_lossy(&output.stdout));
    
    // Offer solutions
    println!("\n{}", "Common solutions:".bright_cyan());
    let solutions = vec![
        "View detailed logs",
        "Describe node resource",
        "Check operator logs",
        "Restart node pods",
        "Delete and recreate node",
        "Back to main menu",
    ];
    
    let solution_idx = Select::with_theme(&ColorfulTheme::default())
        .with_prompt("Select an action")
        .items(&solutions)
        .default(0)
        .interact()?;
    
    match solution_idx {
        0 => {
            let output = Command::new("kubectl")
                .args(&["stellar", "logs", &node_name, "--tail=100"])
                .output()?;
            println!("{}", String::from_utf8_lossy(&output.stdout));
        }
        1 => {
            let output = Command::new("kubectl")
                .args(&["describe", "stellarnode", &node_name])
                .output()?;
            println!("{}", String::from_utf8_lossy(&output.stdout));
        }
        2 => {
            let output = Command::new("kubectl")
                .args(&["logs", "-n", "stellar-system", "-l", "app.kubernetes.io/name=stellar-operator", "--tail=50"])
                .output()?;
            println!("{}", String::from_utf8_lossy(&output.stdout));
        }
        3 => {
            let confirm = Confirm::with_theme(&ColorfulTheme::default())
                .with_prompt(format!("Restart pods for {}?", node_name))
                .default(false)
                .interact()?;
            
            if confirm {
                let output = Command::new("kubectl")
                    .args(&["delete", "pods", "-l", &format!("app.kubernetes.io/instance={}", node_name)])
                    .output()?;
                println!("{}", String::from_utf8_lossy(&output.stdout));
            }
        }
        4 => {
            println!("{}", "⚠ WARNING: This will delete and recreate the node.".bright_red());
            println!("{}", "  Data will be preserved if using persistent volumes.".bright_yellow());
            
            let confirm = Confirm::with_theme(&ColorfulTheme::default())
                .with_prompt("Are you sure?")
                .default(false)
                .interact()?;
            
            if confirm {
                println!("{}", "Deleting node...".bright_yellow());
                Command::new("kubectl")
                    .args(&["delete", "stellarnode", &node_name])
                    .output()?;
                println!("{}", "Node deleted. Please redeploy using the deployment wizard.".bright_green());
            }
        }
        5 => {}
        _ => unreachable!(),
    }
    
    Ok(())
}

/// Scale Horizon deployment wizard
async fn scale_horizon_wizard() -> Result<()> {
    println!("{}", "\n📈 Scale Horizon Deployment".bright_green().bold());
    
    let node_name: String = Input::with_theme(&ColorfulTheme::default())
        .with_prompt("Enter Horizon node name")
        .interact_text()?;
    
    let replicas: u32 = Input::with_theme(&ColorfulTheme::default())
        .with_prompt("Enter desired replica count")
        .default(3)
        .interact_text()?;
    
    let confirm = Confirm::with_theme(&ColorfulTheme::default())
        .with_prompt(format!("Scale {} to {} replicas?", node_name, replicas))
        .default(false)
        .interact()?;
    
    if confirm {
        println!("\n{}", "Scaling deployment...".bright_yellow());
        let output = Command::new("kubectl")
            .args(&["scale", "deployment", &node_name, "--replicas", &replicas.to_string()])
            .output()?;
        
        if output.status.success() {
            println!("{}", "✓ Deployment scaled successfully!".bright_green());
        } else {
            println!("{}", "✗ Scaling failed:".bright_red());
            println!("{}", String::from_utf8_lossy(&output.stderr));
        }
    }
    
    Ok(())
}

/// Backup and restore wizard
async fn backup_restore_wizard() -> Result<()> {
    println!("{}", "\n💾 Backup and Restore".bright_green().bold());
    
    let options = vec!["Create backup", "List backups", "Restore from backup"];
    let selection = Select::with_theme(&ColorfulTheme::default())
        .with_prompt("Select operation")
        .items(&options)
        .default(0)
        .interact()?;
    
    match selection {
        0 => {
            let node_name: String = Input::with_theme(&ColorfulTheme::default())
                .with_prompt("Enter node name to backup")
                .interact_text()?;
            
            println!("\n{}", "Creating VolumeSnapshot...".bright_yellow());
            let output = Command::new("kubectl")
                .args(&["stellar", "snapshot", "create", &node_name])
                .output()?;
            println!("{}", String::from_utf8_lossy(&output.stdout));
        }
        1 => {
            println!("\n{}", "Listing VolumeSnapshots...".bright_yellow());
            let output = Command::new("kubectl")
                .args(&["stellar", "snapshot", "list"])
                .output()?;
            println!("{}", String::from_utf8_lossy(&output.stdout));
        }
        2 => {
            let snapshot_name: String = Input::with_theme(&ColorfulTheme::default())
                .with_prompt("Enter snapshot name")
                .interact_text()?;
            
            let node_name: String = Input::with_theme(&ColorfulTheme::default())
                .with_prompt("Enter node name to restore to")
                .interact_text()?;
            
            let confirm = Confirm::with_theme(&ColorfulTheme::default())
                .with_prompt(format!("Restore {} to {}?", snapshot_name, node_name))
                .default(false)
                .interact()?;
            
            if confirm {
                println!("\n{}", "Restoring from snapshot...".bright_yellow());
                let output = Command::new("kubectl")
                    .args(&["stellar", "snapshot", "restore", &snapshot_name, &node_name])
                    .output()?;
                println!("{}", String::from_utf8_lossy(&output.stdout));
            }
        }
        _ => unreachable!(),
    }
    
    Ok(())
}

/// View logs wizard with filtering
async fn view_logs_wizard() -> Result<()> {
    println!("{}", "\n📜 View Logs".bright_green().bold());
    
    let node_name: String = Input::with_theme(&ColorfulTheme::default())
        .with_prompt("Enter node name")
        .interact_text()?;
    
    let follow = Confirm::with_theme(&ColorfulTheme::default())
        .with_prompt("Follow logs (stream)?")
        .default(false)
        .interact()?;
    
    let tail: u32 = Input::with_theme(&ColorfulTheme::default())
        .with_prompt("Number of lines to show")
        .default(100)
        .interact_text()?;
    
    println!("\n{}", "Fetching logs...".bright_yellow());
    
    let mut args = vec!["stellar", "logs", &node_name, "--tail", &tail.to_string()];
    if follow {
        args.push("--follow");
    }
    
    let output = Command::new("kubectl")
        .args(&args)
        .output()?;
    
    println!("{}", String::from_utf8_lossy(&output.stdout));
    
    Ok(())
}

/// Network diagnostics wizard
async fn network_diagnostics_wizard() -> Result<()> {
    println!("{}", "\n🌐 Network Diagnostics".bright_green().bold());
    
    let options = vec![
        "View network topology",
        "Check peer connections",
        "Test connectivity",
        "View SCP metrics",
    ];
    
    let selection = Select::with_theme(&ColorfulTheme::default())
        .with_prompt("Select diagnostic")
        .items(&options)
        .default(0)
        .interact()?;
    
    match selection {
        0 => {
            println!("\n{}", "Generating network topology...".bright_yellow());
            let output = Command::new("kubectl")
                .args(&["stellar", "topology"])
                .output()?;
            println!("{}", String::from_utf8_lossy(&output.stdout));
        }
        1 => {
            let node_name: String = Input::with_theme(&ColorfulTheme::default())
                .with_prompt("Enter node name")
                .interact_text()?;
            
            println!("\n{}", "Checking peer connections...".bright_yellow());
            let output = Command::new("kubectl")
                .args(&["stellar", "status", &node_name, "-o", "json"])
                .output()?;
            
            // Parse and display peer info
            println!("{}", String::from_utf8_lossy(&output.stdout));
        }
        2 => {
            println!("\n{}", "Testing connectivity...".bright_yellow());
            println!("{}", "This feature is coming soon!".bright_yellow());
        }
        3 => {
            println!("\n{}", "Fetching SCP metrics...".bright_yellow());
            println!("{}", "This feature is coming soon!".bright_yellow());
        }
        _ => unreachable!(),
    }
    
    Ok(())
}

/// Generate a StellarNode manifest
fn generate_node_manifest(
    name: &str,
    node_type: &str,
    namespace: &str,
    network: &str,
    storage_size: &str,
) -> String {
    format!(
        r#"apiVersion: stellar.org/v1alpha1
kind: StellarNode
metadata:
  name: {}
  namespace: {}
spec:
  nodeType: {}
  network: {}
  storage:
    size: {}
    storageClassName: standard
  resources:
    requests:
      cpu: "500m"
      memory: "1Gi"
    limits:
      cpu: "2"
      memory: "4Gi"
"#,
        name, namespace, node_type, network, storage_size
    )
}
