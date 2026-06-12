# Terraform Variables for TigerScan

variable "aws_region" {
  description = "AWS region"
  type        = string
  default     = "us-east-1"
}

variable "environment" {
  description = "Environment name (production, staging, development)"
  type        = string
  default     = "production"
}

variable "db_password" {
  description = "Database password"
  type        = string
  sensitive   = true
}

variable "api_image" {
  description = "Docker image for API"
  type        = string
  default     = "tigerscan/api:latest"
}

variable "api_desired_count" {
  description = "Desired number of API containers"
  type        = number
  default     = 3
}

variable "api_min_count" {
  description = "Minimum number of API containers"
  type        = number
  default     = 2
}

variable "api_max_count" {
  description = "Maximum number of API containers"
  type        = number
  default     = 10
}

variable "acm_certificate_arn" {
  description = "ACM certificate ARN for HTTPS"
  type        = string
  default     = ""
}

variable "vpc_cidr" {
  description = "VPC CIDR block"
  type        = string
  default     = "10.0.0.0/16"
}

variable "availability_zones" {
  description = "Availability zones"
  type        = list(string)
  default     = ["us-east-1a", "us-east-1b", "us-east-1c"]
}