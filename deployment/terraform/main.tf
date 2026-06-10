# Terraform configuration for TigerSmartChain infrastructure
# Deploys Kubernetes cluster and related resources

terraform {
  required_version = ">= 1.0"
  
  required_providers {
    aws = {
      source  = "hashicorp/aws"
      version = "~> 5.0"
    }
    kubernetes = {
      source  = "hashicorp/kubernetes"
      version = "~> 2.0"
    }
    helm = {
      source  = "hashicorp/helm"
      version = "~> 2.0"
    }
  }
  
  backend "s3" {
    bucket = "tigersmartchain-terraform-state"
    key    = "infrastructure/terraform.tfstate"
    region = "us-east-1"
  }
}

# Variables
variable "region" {
  description = "AWS region"
  type        = string
  default     = "us-east-1"
}

variable "cluster_name" {
  description = "EKS cluster name"
  type        = string
  default     = "tigersmartchain"
}

variable "environment" {
  description = "Environment name"
  type        = string
  default     = "production"
}

# AWS Provider
provider "aws" {
  region = var.region
}

# =============================================================================
# NETWORKING
# =============================================================================

# VPC
resource "aws_vpc" "main" {
  cidr_block           = "10.0.0.0/16"
  enable_dns_hostnames = true
  enable_dns_support   = true
  
  tags = {
    Name = "${var.cluster_name}-vpc"
    Environment = var.environment
  }
}

# Internet Gateway
resource "aws_internet_gateway" "main" {
  vpc_id = aws_vpc.main.id
  
  tags = {
    Name = "${var.cluster_name}-igw"
  }
}

# Public Subnets
resource "aws_subnet" "public_1" {
  vpc_id                  = aws_vpc.main.id
  cidr_block             = "10.0.1.0/24"
  availability_zone       = "${var.region}a"
  map_public_ip_on_launch = true
  
  tags = {
    Name = "${var.cluster_name}-public-1"
    Type = "public"
  }
}

resource "aws_subnet" "public_2" {
  vpc_id                  = aws_vpc.main.id
  cidr_block             = "10.0.2.0/24"
  availability_zone       = "${var.region}b"
  map_public_ip_on_launch = true
  
  tags = {
    Name = "${var.cluster_name}-public-2"
    Type = "public"
  }
}

# Private Subnets
resource "aws_subnet" "private_1" {
  vpc_id            = aws_vpc.main.id
  cidr_block       = "10.0.10.0/24"
  availability_zone = "${var.region}a"
  
  tags = {
    Name = "${var.cluster_name}-private-1"
    Type = "private"
  }
}

resource "aws_subnet" "private_2" {
  vpc_id            = aws_vpc.main.id
  cidr_block       = "10.0.11.0/24"
  availability_zone = "${var.region}b"
  
  tags = {
    Name = "${var.cluster_name}-private-2"
    Type = "private"
  }
}

# Elastic IP for NAT Gateway
resource "aws_eip" "nat" {
  domain = "vpc"
  
  tags = {
    Name = "${var.cluster_name}-nat-eip"
  }
}

# NAT Gateway
resource "aws_nat_gateway" "main" {
  allocation_id = aws_eip.nat.id
  subnet_id     = aws_subnet.public_1.id
  
  tags = {
    Name = "${var.cluster_name}-nat"
  }
  
  depends_on = [aws_internet_gateway.main]
}

# Route Tables
resource "aws_route_table" "public" {
  vpc_id = aws_vpc.main.id
  
  route {
    cidr_block = "0.0.0.0/0"
    gateway_id = aws_internet_gateway.main.id
  }
  
  tags = {
    Name = "${var.cluster_name}-public-rt"
  }
}

resource "aws_route_table" "private" {
  vpc_id = aws_vpc.main.id
  
  route {
    cidr_block    = "0.0.0.0/0"
    nat_gateway_id = aws_nat_gateway.main.id
  }
  
  tags = {
    Name = "${var.cluster_name}-private-rt"
  }
}

# Route Table Associations
resource "aws_route_table_association" "public_1" {
  subnet_id      = aws_subnet.public_1.id
  route_table_id = aws_route_table.public.id
}

resource "aws_route_table_association" "public_2" {
  subnet_id      = aws_subnet.public_2.id
  route_table_id = aws_route_table.public.id
}

resource "aws_route_table_association" "private_1" {
  subnet_id      = aws_subnet.private_1.id
  route_table_id = aws_route_table.private.id
}

resource "aws_route_table_association" "private_2" {
  subnet_id      = aws_subnet.private_2.id
  route_table_id = aws_route_table.private.id
}

# =============================================================================
# EKS CLUSTER
# =============================================================================

# EKS Cluster
resource "aws_eks_cluster" "main" {
  name     = var.cluster_name
  role_arn = aws_iam_role.eks_cluster.arn
  version  = "1.28"
  
  vpc_config {
    subnet_ids = [
      aws_subnet.public_1.id,
      aws_subnet.public_2.id,
      aws_subnet.private_1.id,
      aws_subnet.private_2.id
    ]
    
    endpoint_private_access = true
    endpoint_public_access  = true
    
    public_access_cidrs = ["0.0.0.0/0"]
  }
  
  depends_on = [
    aws_iam_role_policy_attachment.eks_cluster_policy
  ]
  
  tags = {
    Name        = var.cluster_name
    Environment = var.environment
  }
}

# EKS Node Group - Validators
resource "aws_eks_node_group" "validators" {
  cluster_name    = aws_eks_cluster.main.name
  node_group_name = "${var.cluster_name}-validators"
  node_role_arn   = aws_iam_role.eks_nodes.arn
  subnet_ids      = [aws_subnet.private_1.id, aws_subnet.private_2.id]
  
  scaling_config {
    desired_size = 4
    max_size     = 10
    min_size     = 1
  }
  
  instance_types = ["c5.2xlarge"]
  
  labels = {
    role = "validator"
  }
  
  tags = {
    Name = "${var.cluster_name}-validators"
  }
}

# EKS Node Group - Full Nodes
resource "aws_eks_node_group" "fullnodes" {
  cluster_name    = aws_eks_cluster.main.name
  node_group_name = "${var.cluster_name}-fullnodes"
  node_role_arn   = aws_iam_role.eks_nodes.arn
  subnet_ids      = [aws_subnet.private_1.id, aws_subnet.private_2.id]
  
  scaling_config {
    desired_size = 2
    max_size     = 5
    min_size     = 1
  }
  
  instance_types = ["c5.xlarge"]
  
  labels = {
    role = "fullnode"
  }
  
  tags = {
    Name = "${var.cluster_name}-fullnodes"
  }
}

# =============================================================================
# IAM ROLES
# =============================================================================

# EKS Cluster Role
resource "aws_iam_role" "eks_cluster" {
  name = "${var.cluster_name}-eks-cluster"
  
  assume_role_policy = jsonencode({
    Version = "2012-10-17"
    Statement = [{
      Action = "sts:AssumeRole"
      Effect = "Allow"
      Principal = {
        Service = "eks.amazonaws.com"
      }
    }]
  })
}

resource "aws_iam_role_policy_attachment" "eks_cluster_policy" {
  policy_arn = "arn:aws:iam::aws:policy/AmazonEKSClusterPolicy"
  role       = aws_iam_role.eks_cluster.name
}

# EKS Nodes Role
resource "aws_iam_role" "eks_nodes" {
  name = "${var.cluster_name}-eks-nodes"
  
  assume_role_policy = jsonencode({
    Version = "2012-10-17"
    Statement = [{
      Action = "sts:AssumeRole"
      Effect = "Allow"
      Principal = {
        Service = "ec2.amazonaws.com"
      }
    }]
  })
}

resource "aws_iam_role_policy_attachment" "eks_worker_node" {
  policy_arn = "arn:aws:iam::aws:policy/AmazonEKSWorkerNodePolicy"
  role       = aws_iam_role.eks_nodes.name
}

resource "aws_iam_role_policy_attachment" "eks_cni" {
  policy_arn = "arn:aws:iam::aws:policy/AmazonEKS_CNI_Policy"
  role       = aws_iam_role.eks_nodes.name
}

resource "aws_iam_role_policy_attachment" "eks_container_registry" {
  policy_arn = "arn:aws:iam::aws:policy/AmazonEC2ContainerRegistryReadOnly"
  role       = aws_iam_role.eks_nodes.name
}

# =============================================================================
# KUBERNETES PROVIDER
# =============================================================================

provider "kubernetes" {
  host                   = aws_eks_cluster.main.endpoint
  cluster_ca_certificate = base64decode(aws_eks_cluster.main.certificate_authority[0].data)
  token                 = data.aws_eks_cluster_auth.main.token
}

data "aws_eks_cluster_auth" "main" {
  name = aws_eks_cluster.main.name
}

# =============================================================================
# HELM RELEASE
# =============================================================================

provider "helm" {
  kubernetes {
    host                   = aws_eks_cluster.main.endpoint
    cluster_ca_certificate = base64decode(aws_eks_cluster.main.certificate_authority[0].data)
    token                 = data.aws_eks_cluster_auth.main.token
  }
}

resource "helm_release" "tigersmartchain" {
  name       = "tigersmartchain"
  repository = "https://tigersmartchain.github.io/helm"
  chart      = "tigersmartchain"
  version    = "1.0.0"
  
  set {
    name  = "global.chain.id"
    value = "9001"
  }
  
  set {
    name  = "validator.replicas"
    value = "4"
  }
  
  set {
    name  = "fullnode.replicas"
    value = "2"
  }
  
  set {
    name  = "monitoring.enabled"
    value = "true"
  }
}

# =============================================================================
# OUTPUTS
# =============================================================================

output "cluster_name" {
  description = "EKS cluster name"
  value      = aws_eks_cluster.main.name
}

output "cluster_endpoint" {
  description = "EKS cluster endpoint"
  value      = aws_eks_cluster.main.endpoint
}

output "cluster_arn" {
  description = "EKS cluster ARN"
  value      = aws_eks_cluster.main.arn
}

output "vpc_id" {
  description = "VPC ID"
  value      = aws_vpc.main.id
}