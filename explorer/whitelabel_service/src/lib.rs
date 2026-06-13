//! White-label Service - Custom Branding Solution

use std::collections::HashMap;
use std::sync::Arc;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WhiteLabelConfig {
    pub id: String,
    pub name: String,
    pub domain: String,
    pub branding: BrandingConfig,
    pub features: FeatureFlags,
    pub custom_domain: Option<String>,
    pub api_keys: Vec<String>,
    pub created_at: i64,
    pub expires_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrandingConfig {
    pub logo: String,
    pub logo_dark: String,
    pub favicon: String,
    pub primary_color: String,
    pub secondary_color: String,
    pub accent_color: String,
    pub font_family: String,
    pub custom_css: Option<String>,
    pub header_text: String,
    pub footer_text: String,
    pub support_email: String,
    pub social_links: SocialLinks,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SocialLinks {
    pub twitter: Option<String>,
    pub telegram: Option<String>,
    pub discord: Option<String>,
    pub github: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureFlags {
    pub custom_domain: bool,
    pub api_access: bool,
    pub analytics: bool,
    pub white_label: bool,
    pub priority_support: bool,
    pub dedicated_infrastructure: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WhiteLabelInstance {
    pub config: WhiteLabelConfig,
    pub status: InstanceStatus,
    pub dashboard_url: String,
    pub api_url: String,
    pub custom_domains: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InstanceStatus {
    Pending,
    Provisioning,
    Active,
    Suspended,
    Expired,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstanceRequest {
    pub name: String,
    pub domain: String,
    pub plan: PricingPlan,
    pub custom_branding: Option<BrandingConfig>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PricingPlan {
    Starter,
    Professional,
    Enterprise,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditReport {
    pub id: String,
    pub instance_id: String,
    pub report_type: ReportType,
    pub findings: Vec<Finding>,
    pub created_at: i64,
    pub expires_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Finding {
    pub severity: Severity,
    pub category: String,
    pub description: String,
    pub recommendation: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReportType {
    Security,
    Performance,
    Compliance,
    Full,
}

pub struct WhiteLabelService {
    instances: Arc<RwLock<HashMap<String, WhiteLabelInstance>>>,
    templates: Arc<RwLock<HashMap<String, BrandingConfig>>>,
}

impl WhiteLabelService {
    pub fn new() -> Self {
        Self {
            instances: Arc::new(RwLock::new(HashMap::new())),
            templates: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Initialize with default templates
    pub fn init_templates(&self) {
        let mut templates = self.templates.write();
        
        templates.insert("default".to_string(), BrandingConfig {
            logo: "/images/logo.svg".to_string(),
            logo_dark: "/images/logo-dark.svg".to_string(),
            favicon: "/favicon.ico".to_string(),
            primary_color: "#ff6b35".to_string(),
            secondary_color: "#12121a".to_string(),
            accent_color: "#00cc88".to_string(),
            font_family: "Inter, system-ui, sans-serif".to_string(),
            custom_css: None,
            header_text: "TigerScan".to_string(),
            footer_text: "© 2024 TigerScan. All rights reserved.".to_string(),
            support_email: "support@tigerscan.io".to_string(),
            social_links: SocialLinks {
                twitter: Some("https://twitter.com/tigerscan".to_string()),
                telegram: Some("https://t.me/tigerscan".to_string()),
                discord: None,
                github: Some("https://github.com/tigerscan".to_string()),
            },
        });
    }

    /// Create new white-label instance
    pub fn create_instance(&self, request: InstanceRequest) -> WhiteLabelInstance {
        let id = Uuid::new_v4().to_string();
        
        let branding = request.custom_branding.unwrap_or_else(|| {
            self.templates.read().get("default").cloned().unwrap_or_else(|| BrandingConfig {
                logo: "/images/logo.svg".to_string(),
                logo_dark: "/images/logo-dark.svg".to_string(),
                favicon: "/favicon.ico".to_string(),
                primary_color: "#ff6b35".to_string(),
                secondary_color: "#12121a".to_string(),
                accent_color: "#00cc88".to_string(),
                font_family: "Inter, system-ui, sans-serif".to_string(),
                custom_css: None,
                header_text: request.name.clone(),
                footer_text: format!("© 2024 {}. All rights reserved.", request.name),
                support_email: "support@example.com".to_string(),
                social_links: SocialLinks {
                    twitter: None,
                    telegram: None,
                    discord: None,
                    github: None,
                },
            })
        });
        
        let config = WhiteLabelConfig {
            id: id.clone(),
            name: request.name,
            domain: request.domain,
            branding,
            features: FeatureFlags {
                custom_domain: matches!(request.plan, PricingPlan::Professional | PricingPlan::Enterprise),
                api_access: true,
                analytics: true,
                white_label: true,
                priority_support: matches!(request.plan, PricingPlan::Enterprise),
                dedicated_infrastructure: matches!(request.plan, PricingPlan::Enterprise),
            },
            custom_domain: None,
            api_keys: vec![],
            created_at: chrono::Utc::now().timestamp(),
            expires_at: None,
        };
        
        let instance = WhiteLabelInstance {
            config,
            status: InstanceStatus::Active,
            dashboard_url: format!("https://{}.tigerscan.io", id),
            api_url: format!("https://api.{}.tigerscan.io", id),
            custom_domains: vec![],
        };
        
        let mut instances = self.instances.write();
        instances.insert(id, instance.clone());
        
        instance
    }

    /// Get instance
    pub fn get_instance(&self, id: &str) -> Option<WhiteLabelInstance> {
        self.instances.read().get(id).cloned()
    }

    /// Update instance
    pub fn update_instance(&self, id: &str, branding: BrandingConfig) -> Result<WhiteLabelInstance, String> {
        let mut instances = self.instances.write();
        
        let instance = instances.get_mut(id)
            .ok_or("Instance not found")?;
        
        instance.config.branding = branding;
        
        Ok(instance.clone())
    }

    /// Generate audit report
    pub fn generate_audit_report(&self, id: &str, report_type: ReportType) -> Result<AuditReport, String> {
        let _ = self.instances.read().get(id)
            .ok_or("Instance not found")?;
        
        let findings = match report_type {
            ReportType::Security => vec![
                Finding { severity: Severity::Low, category: "SSL".to_string(), description: "SSL certificate valid".to_string(), recommendation: "Continue monitoring".to_string() },
                Finding { severity: Severity::Medium, category: "Headers".to_string(), description: "Missing HSTS header".to_string(), recommendation: "Add Strict-Transport-Security header".to_string() },
            ],
            ReportType::Performance => vec![
                Finding { severity: Severity::Low, category: "CDN".to_string(), description: "Using CloudFront CDN".to_string(), recommendation: "Good".to_string() },
            ],
            ReportType::Compliance => vec![
                Finding { severity: Severity::Low, category: "GDPR".to_string(), description: "Privacy policy present".to_string(), recommendation: "Good".to_string() },
            ],
            ReportType::Full => vec![
                Finding { severity: Severity::Low, category: "SSL".to_string(), description: "SSL certificate valid".to_string(), recommendation: "Continue monitoring".to_string() },
                Finding { severity: Severity::Medium, category: "Headers".to_string(), description: "Missing HSTS header".to_string(), recommendation: "Add Strict-Transport-Security header".to_string() },
                Finding { severity: Severity::Low, category: "CDN".to_string(), description: "Using CloudFront CDN".to_string(), recommendation: "Good".to_string() },
                Finding { severity: Severity::Low, category: "GDPR".to_string(), description: "Privacy policy present".to_string(), recommendation: "Good".to_string() },
            ],
        };
        
        Ok(AuditReport {
            id: Uuid::new_v4().to_string(),
            instance_id: id.to_string(),
            report_type,
            findings,
            created_at: chrono::Utc::now().timestamp(),
            expires_at: chrono::Utc::now().timestamp() + 86400 * 365,
        })
    }

    /// List all instances
    pub fn list_instances(&self) -> Vec<WhiteLabelInstance> {
        self.instances.read().values().cloned().collect()
    }

    /// Suspend instance
    pub fn suspend_instance(&self, id: &str) -> Result<(), String> {
        let mut instances = self.instances.write();
        
        let instance = instances.get_mut(id)
            .ok_or("Instance not found")?;
        
        instance.status = InstanceStatus::Suspended;
        
        Ok(())
    }

    /// Activate instance
    pub fn activate_instance(&self, id: &str) -> Result<(), String> {
        let mut instances = self.instances.write();
        
        let instance = instances.get_mut(id)
            .ok_or("Instance not found")?;
        
        instance.status = InstanceStatus::Active;
        
        Ok(())
    }
}

// Generate CSS from branding
pub fn generate_css(branding: &BrandingConfig) -> String {
    format!(r#"
:root {{
    --primary: {};
    --secondary: {};
    --accent: {};
    --font-family: {};
}}
.logo {{ content: url({}); }}
"#, 
        branding.primary_color,
        branding.secondary_color,
        branding.accent_color,
        branding.font_family,
        branding.logo
    )
}