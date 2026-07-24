//! Network zone resolution — ported from rustset utils.
//!
//! Maps IP addresses to network zones based on CIDR rules.

use std::net::IpAddr;

use ipnetwork::IpNetwork;
use kairos_platform_scan_api::{NetworkZone, ZoneConfig};

/// Determine the network zone for a given IP address.
pub fn determine_zone(ip_str: &str, zones: &[ZoneConfig]) -> NetworkZone {
    let Ok(ip) = ip_str.parse::<IpAddr>() else {
        return NetworkZone::Internet;
    };

    let mut matching_zones: Vec<&ZoneConfig> = zones
        .iter()
        .filter(|z| {
            if let Ok(net) = z.cidr.parse::<IpNetwork>() {
                net.contains(ip)
            } else {
                false
            }
        })
        .collect();

    matching_zones.sort_by(|a, b| b.priority.cmp(&a.priority));

    if let Some(best_match) = matching_zones.first() {
        return match best_match.name.as_str() {
            "Internet" => NetworkZone::Internet,
            "DMZ" => NetworkZone::Dmz,
            "Intranet" => NetworkZone::Intranet,
            other => NetworkZone::Custom(other.to_string()),
        };
    }

    NetworkZone::Internet
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_zones() -> Vec<ZoneConfig> {
        vec![
            ZoneConfig {
                id: "1".into(),
                name: "Intranet".into(),
                cidr: "10.0.0.0/8".into(),
                priority: 100,
                cloud_platform_id: None,
                cloud_platform_name: None,
                machine_room_id: None,
                machine_room_name: None,
            },
            ZoneConfig {
                id: "2".into(),
                name: "DMZ".into(),
                cidr: "192.168.1.0/24".into(),
                priority: 100,
                cloud_platform_id: None,
                cloud_platform_name: None,
                machine_room_id: None,
                machine_room_name: None,
            },
        ]
    }

    #[test]
    fn test_determine_zone_internet() {
        let zones = test_zones();
        let result = determine_zone("8.8.8.8", &zones);
        assert!(matches!(result, NetworkZone::Internet));
    }

    #[test]
    fn test_determine_zone_intranet() {
        let zones = test_zones();
        let result = determine_zone("10.0.1.5", &zones);
        assert!(matches!(result, NetworkZone::Intranet));
    }

    #[test]
    fn test_determine_zone_dmz() {
        let zones = test_zones();
        let result = determine_zone("192.168.1.10", &zones);
        assert!(matches!(result, NetworkZone::Dmz));
    }

    #[test]
    fn test_determine_zone_invalid_ip() {
        let zones = test_zones();
        let result = determine_zone("invalid-ip", &zones);
        assert!(matches!(result, NetworkZone::Internet));
    }

    #[test]
    fn test_determine_zone_priority() {
        let zones = vec![
            ZoneConfig {
                id: "1".into(),
                name: "LowPriority".into(),
                cidr: "10.0.0.0/8".into(),
                priority: 50,
                cloud_platform_id: None,
                cloud_platform_name: None,
                machine_room_id: None,
                machine_room_name: None,
            },
            ZoneConfig {
                id: "2".into(),
                name: "HighPriority".into(),
                cidr: "10.0.0.0/8".into(),
                priority: 100,
                cloud_platform_id: None,
                cloud_platform_name: None,
                machine_room_id: None,
                machine_room_name: None,
            },
        ];
        let result = determine_zone("10.0.1.5", &zones);
        assert!(matches!(result, NetworkZone::Custom(ref name) if name == "HighPriority"));
    }
}
