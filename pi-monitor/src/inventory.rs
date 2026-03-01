/// Parse an Ansible YAML inventory file into a flat list of nodes.
///
/// Handles the standard structure:
///   all:
///     hosts:
///       name:
///         ansible_host: hostname-or-ip
///     children:
///       groupname:
///         hosts:
///           name:

use std::collections::HashMap;
use std::path::Path;

use anyhow::{Context, Result};
use serde_yaml::Value;

#[derive(Debug, Clone)]
pub struct InventoryNode {
    /// Ansible alias (e.g. "control", "worker1")
    pub name: String,
    /// The address to connect to (e.g. "control.local", "192.168.1.10")
    pub ansible_host: String,
    /// Child groups this host belongs to (empty for top-level-only hosts)
    pub groups: Vec<String>,
}

pub fn parse(path: &Path) -> Result<Vec<InventoryNode>> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("cannot read inventory file: {}", path.display()))?;

    let doc: Value = serde_yaml::from_str(&content)
        .with_context(|| "inventory is not valid YAML")?;

    let all = &doc["all"];

    // ── Collect ansible_host for every host in all.hosts ────────────────────
    let mut host_addr: HashMap<String, String> = HashMap::new();
    if let Some(hosts) = all["hosts"].as_mapping() {
        for (k, v) in hosts {
            let name = k.as_str().unwrap_or("").to_string();
            if name.is_empty() {
                continue;
            }
            // ansible_host may be absent (fall back to the alias name)
            let addr = v["ansible_host"]
                .as_str()
                .unwrap_or(&name)
                .to_string();
            host_addr.insert(name, addr);
        }
    }

    // ── Collect group membership from all.children ───────────────────────────
    // host name → list of groups
    let mut host_groups: HashMap<String, Vec<String>> = HashMap::new();
    if let Some(children) = all["children"].as_mapping() {
        for (group_key, group_val) in children {
            let group_name = group_key.as_str().unwrap_or("").to_string();
            if group_name.is_empty() {
                continue;
            }
            if let Some(group_hosts) = group_val["hosts"].as_mapping() {
                for (hk, hv) in group_hosts {
                    let hname = hk.as_str().unwrap_or("").to_string();
                    if hname.is_empty() {
                        continue;
                    }
                    // If the host has an ansible_host override at the group level, record it
                    if let Some(addr) = hv["ansible_host"].as_str() {
                        host_addr.entry(hname.clone()).or_insert_with(|| addr.to_string());
                    }
                    // If this host isn't yet in host_addr at all, fall back to alias
                    host_addr.entry(hname.clone()).or_insert_with(|| hname.clone());
                    host_groups.entry(hname).or_default().push(group_name.clone());
                }
            }
        }
    }

    // ── Build the flat list, preserving insertion order ──────────────────────
    // Use the order from all.hosts first, then any hosts only seen in children.
    let mut seen: Vec<String> = Vec::new();
    if let Some(hosts) = all["hosts"].as_mapping() {
        for k in hosts.keys() {
            if let Some(name) = k.as_str() {
                seen.push(name.to_string());
            }
        }
    }
    // Append hosts only found in children groups
    for name in host_groups.keys() {
        if !seen.contains(name) {
            seen.push(name.clone());
        }
    }

    let nodes = seen
        .into_iter()
        .filter_map(|name| {
            let addr = host_addr.get(&name)?.clone();
            let groups = host_groups.get(&name).cloned().unwrap_or_default();
            Some(InventoryNode {
                name,
                ansible_host: addr,
                groups,
            })
        })
        .collect();

    Ok(nodes)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
all:
  vars:
    ansible_python_interpreter: /usr/bin/python3
  hosts:
    control:
      ansible_host: control.local
    worker1:
      ansible_host: worker1.local
    worker2:
      ansible_host: worker2.local
  children:
    workers:
      hosts:
        worker1:
        worker2:
"#;

    fn parse_str(yaml: &str) -> Vec<InventoryNode> {
        let tmp = tempfile(yaml);
        parse(&tmp).expect("parse failed")
    }

    fn tempfile(content: &str) -> std::path::PathBuf {
        use std::io::Write;
        let path = std::env::temp_dir().join("pi-monitor-test-inventory.yml");
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(content.as_bytes()).unwrap();
        path
    }

    #[test]
    fn parses_all_hosts() {
        let nodes = parse_str(SAMPLE);
        assert_eq!(nodes.len(), 3);
        let names: Vec<_> = nodes.iter().map(|n| n.name.as_str()).collect();
        assert!(names.contains(&"control"));
        assert!(names.contains(&"worker1"));
        assert!(names.contains(&"worker2"));
    }

    #[test]
    fn ansible_host_values() {
        let nodes = parse_str(SAMPLE);
        let control = nodes.iter().find(|n| n.name == "control").unwrap();
        assert_eq!(control.ansible_host, "control.local");
        let w1 = nodes.iter().find(|n| n.name == "worker1").unwrap();
        assert_eq!(w1.ansible_host, "worker1.local");
    }

    #[test]
    fn group_membership() {
        let nodes = parse_str(SAMPLE);
        let control = nodes.iter().find(|n| n.name == "control").unwrap();
        assert!(control.groups.is_empty(), "control should have no child groups");
        let w1 = nodes.iter().find(|n| n.name == "worker1").unwrap();
        assert!(w1.groups.contains(&"workers".to_string()));
        let w2 = nodes.iter().find(|n| n.name == "worker2").unwrap();
        assert!(w2.groups.contains(&"workers".to_string()));
    }

    #[test]
    fn no_duplicate_nodes() {
        let nodes = parse_str(SAMPLE);
        // worker1 appears in both all.hosts and children.workers.hosts
        let count = nodes.iter().filter(|n| n.name == "worker1").count();
        assert_eq!(count, 1);
    }
}
