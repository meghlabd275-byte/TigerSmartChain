#![forbid(unsafe_code)]
use whitelabel_service::{WhiteLabelService, InstanceRequest, PricingPlan};

fn main() {
    println!("Starting White-label Service");
    
    let service = WhiteLabelService::new();
    service.init_templates();
    
    // Example: Create a custom instance
    let request = InstanceRequest {
        name: "MyChain".to_string(),
        domain: "mychain.io".to_string(),
        plan: PricingPlan::Professional,
        custom_branding: None,
    };
    
    let instance = service.create_instance(request);
    println!("Created instance: {}", instance.config.id);
    println!("Dashboard URL: {}", instance.dashboard_url);
    println!("API URL: {}", instance.api_url);
    
    // Generate audit report
    let report = service.generate_audit_report(&instance.config.id, whitelabel_service::ReportType::Full).unwrap();
    println!("\nAudit Report: {} findings", report.findings.len());
    
    println!("\nWhite-label Service running on port 9008");
}