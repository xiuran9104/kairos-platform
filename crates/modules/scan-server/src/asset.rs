//! Asset management helpers — ported from rustset.
//!
//! Asset import/export and port binding utilities.

use kairos_platform_scan_api::{Asset, PortInfo};

/// Merge scan results into an existing asset's port list.
/// Replaces open ports with new scan data; preserves bound port metadata.
pub fn merge_scan_ports(asset: &mut Asset, mut scan_ports: Vec<PortInfo>) {
    // Mark existing ports as closed, preserving bound ones
    for existing_port in asset.ports.iter_mut() {
        if !existing_port.is_bound {
            existing_port.is_open = false;
        }
    }

    // Add or update scanned ports
    for scan_port in scan_ports.drain(..) {
        if let Some(existing) = asset.ports.iter_mut().find(|p| p.port == scan_port.port) {
            // Update existing port; preserve binding info if already bound
            if !existing.is_bound {
                existing.is_open = true;
                existing.service = scan_port.service;
                existing.version = scan_port.version;
                existing.banner = scan_port.banner;
            }
        } else {
            asset.ports.push(scan_port);
        }
    }
}

/// Calculate an asset's risk score based on open ports and weight.
pub fn asset_risk_score(asset: &Asset) -> u32 {
    let open_port_count = asset.ports.iter().filter(|p| p.is_open).count() as u32;
    let base = open_port_count * 5;
    // Weight multiplier: 0.0 .. 2.0
    let multiplier = 1.0 + (asset.weight as f32 / 100.0);
    (base as f32 * multiplier) as u32
}

/// Filter assets by search keyword (matches name, IP, owner, labels).
pub fn filter_assets<'a>(assets: &'a [Asset], keyword: &str) -> Vec<&'a Asset> {
    let keyword = keyword.to_lowercase();
    assets
        .iter()
        .filter(|a| {
            a.name.to_lowercase().contains(&keyword)
                || a.ip.contains(&keyword)
                || a.owner
                    .as_ref()
                    .is_some_and(|o| o.to_lowercase().contains(&keyword))
                || a.labels.iter().any(|l| l.to_lowercase().contains(&keyword))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use kairos_platform_scan_api::NetworkZone;

    fn test_asset() -> Asset {
        Asset {
            id: Some(1),
            name: "web-server".into(),
            ip: "10.0.0.1".into(),
            zone: NetworkZone::Intranet,
            ports: vec![
                PortInfo {
                    port: 80,
                    is_open: true,
                    service: Some("HTTP".into()),
                    version: None,
                    banner: None,
                    is_bound: false,
                    system_name: None,
                    middleware: None,
                },
                PortInfo {
                    port: 443,
                    is_open: true,
                    service: Some("HTTPS".into()),
                    version: None,
                    banner: None,
                    is_bound: true,
                    system_name: Some("Web Portal".into()),
                    middleware: Some("nginx".into()),
                },
            ],
            last_scanned: None,
            contact_person: None,
            contact_phone: None,
            owner: Some("ops-team".into()),
            weight: 80,
            labels: vec!["production".into(), "frontend".into()],
            os: Some("Ubuntu 22.04".into()),
            device_type: Some("VM".into()),
        }
    }

    #[test]
    fn test_merge_scan_preserves_bound_ports() {
        let mut asset = test_asset();
        let scan_ports = vec![PortInfo {
            port: 443,
            is_open: true,
            service: Some("HTTPS".into()),
            version: Some("1.1".into()),
            banner: None,
            is_bound: false,
            system_name: None,
            middleware: None,
        }];
        merge_scan_ports(&mut asset, scan_ports);
        let port_443 = asset.ports.iter().find(|p| p.port == 443).unwrap();
        assert!(port_443.is_bound);
        assert_eq!(port_443.system_name.as_deref(), Some("Web Portal"));
    }

    #[test]
    fn test_asset_risk_score() {
        let asset = test_asset();
        let score = asset_risk_score(&asset);
        assert!(score > 0);
    }

    #[test]
    fn test_filter_assets_by_keyword() {
        let assets = vec![test_asset()];
        assert_eq!(filter_assets(&assets, "web").len(), 1);
        assert_eq!(filter_assets(&assets, "ops-team").len(), 1);
        assert_eq!(filter_assets(&assets, "frontend").len(), 1);
        assert_eq!(filter_assets(&assets, "nonexistent").len(), 0);
    }
}
