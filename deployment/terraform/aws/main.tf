# Terraform Configuration for TigerScan on AWS

terraform {
  required_version = ">= 1.0.0"
  
  required_providers {
    aws = {
      source  = "hashicorp/aws"
      version = "~> 5.0"
    }
  }
}

# Configure AWS provider
provider "aws" {
  region = var.aws_region
}

# Variables
variable "aws_region" {
  description = "AWS region"
  type        = string
  default     = "us-east-1"
}

variable "environment" {
  description = "Environment name"
  type        = string
  default     = "production"
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

# VPC
resource "aws_vpc" "main" {
  cidr_block           = var.vpc_cidr
  enable_dns_hostnames = true
  enable_dns_support   = true
  
  tags = {
    Name        = "tigerscan-vpc-${var.environment}"
    Environment = var.environment
  }
}

# Internet Gateway
resource "aws_internet_gateway" "main" {
  vpc_id = aws_vpc.main.id
  
  tags = {
    Name        = "tigerscan-igw-${var.environment}"
    Environment = var.environment
  }
}

# Public Subnets
resource "aws_subnet" "public" {
  count             = 3
  vpc_id            = aws_vpc.main.id
  cidr_block        = cidrsubnet(var.vpc_cidr, 8, count.index)
  availability_zone = var.availability_zones[count.index]
  
  map_public_ip_on_launch = true
  
  tags = {
    Name        = "tigerscan-public-${count.index + 1}"
    Environment = var.environment
  }
}

# Private Subnets
resource "aws_subnet" "private" {
  count             = 3
  vpc_id            = aws_vpc.main.id
  cidr_block        = cidrsubnet(var.vpc_cidr, 8, count.index + 3)
  availability_zone = var.availability_zones[count.index]
  
  tags = {
    Name        = "tigerscan-private-${count.index + 1}"
    Environment = var.environment
  }
}

# Database Subnets
resource "aws_subnet" "database" {
  count             = 3
  vpc_id            = aws_vpc.main.id
  cidr_block        = cidrsubnet(var.vpc_cidr, 8, count.index + 6)
  availability_zone = var.availability_zones[count.index]
  
  tags = {
    Name        = "tigerscan-database-${count.index + 1}"
    Environment = var.environment
  }
}

# Elasticache Subnets
resource "aws_subnet" "elasticache" {
  count             = 2
  vpc_id            = aws_vpc.main.id
  cidr_block        = cidrsubnet(var.vpc_cidr, 8, count.index + 9)
  availability_zone = var.availability_zones[count.index]
  
  tags = {
    Name        = "tigerscan-elasticache-${count.index + 1}"
    Environment = var.environment
  }
}

# NAT Gateway
resource "aws_nat_gateway" "main" {
  count             = 2
  allocation_id    = aws_eip.nat[count.index].id
  subnet_id        = aws_subnet.public[count.index].id
  
  tags = {
    Name        = "tigerscan-nat-${count.index + 1}"
    Environment = var.environment
  }
  
  depends_on = [aws_internet_gateway.main]
}

# EIP for NAT
resource "aws_eip" "nat" {
  count  = 2
  domain = "vpc"
  
  tags = {
    Name        = "tigerscan-nat-eip-${count.index + 1}"
    Environment = var.environment
  }
}

# Route Table for Public
resource "aws_route_table" "public" {
  vpc_id = aws_vpc.main.id
  
  route {
    cidr_block = "0.0.0.0/0"
    gateway_id = aws_internet_gateway.main.id
  }
  
  tags = {
    Name        = "tigerscan-public-rt"
    Environment = var.environment
  }
}

# Route Table for Private
resource "aws_route_table" "private" {
  count = 2
  vpc_id = aws_vpc.main.id
  
  route {
    cidr_block     = "0.0.0.0/0"
    nat_gateway_id = aws_nat_gateway.main[count.index].id
  }
  
  tags = {
    Name        = "tigerscan-private-rt-${count.index + 1}"
    Environment = var.environment
  }
}

# Route Table Association for Public
resource "aws_route_table_association" "public" {
  count          = 3
  subnet_id      = aws_subnet.public[count.index].id
  route_table_id = aws_route_table.public.id
}

# Route Table Association for Private
resource "aws_route_table_association" "private" {
  count          = 3
  subnet_id      = aws_subnet.private[count.index].id
  route_table_id = aws_route_table.private[count.index % 2].id
}

# RDS Subnet Group
resource "aws_db_subnet_group" "main" {
  name       = "tigerscan-db-subnet-${var.environment}"
  subnet_ids = aws_subnet.database[*].id
  
  tags = {
    Name        = "tigerscan-db-subnet-group"
    Environment = var.environment
  }
}

# ElastiCache Subnet Group
resource "aws_elasticache_subnet_group" "main" {
  name       = "tigerscan-cache-subnet-${var.environment}"
  subnet_ids = aws_subnet.elasticache[*].id
  
  tags = {
    Name        = "tigerscan-cache-subnet-group"
    Environment = var.environment
  }
}

# RDS Instance - PostgreSQL
resource "aws_db_instance" "main" {
  identifier           = "tigerscan-db-${var.environment}"
  engine              = "postgres"
  engine_version     = "15.4"
  instance_class     = "db.r6g.xlarge"
  allocated_storage  = 100
  max_allocated_storage = 1000
  storage_encrypted  = true
  storage_type      = "gp3"
  
  db_name  = "tigerscan"
  username = "tigerscan_admin"
  password = var.db_password
  
  db_subnet_group_name   = aws_db_subnet_group.main.name
  vpc_security_group_ids = [aws_security_group.database.id]
  
  backup_retention_period = 7
  backup_window          = "03:00-04:00"
  maintenance_window     = "mon:04:00-mon:05:00"
  
  skip_final_snapshot       = true
  deletion_protection   = var.environment == "production" ? true : false
  
  tags = {
    Name        = "tigerscan-db"
    Environment = var.environment
  }
}

# ElastiCache Redis
resource "aws_elasticache_cluster" "main" {
  cluster_id           = "tigerscan-cache-${var.environment}"
  engine             = "redis"
  engine_version    = "7.0"
  node_type          = "cache.r6g.large"
  num_cache_nodes    = 2
  parameter_group_name = "default.redis7.0"
  
  port                = 6379
  security_group_ids  = [aws_security_group.redis.id]
  subnet_group_name  = aws_elasticache_subnet_group.main.name
  
  snapshot_retention_limit = 7
  
  tags = {
    Name        = "tigerscan-cache"
    Environment = var.environment
  }
}

# ALB Security Group
resource "aws_security_group" "alb" {
  name        = "tigerscan-alb-sg-${var.environment}"
  description = "Security group for TigerScan ALB"
  vpc_id     = aws_vpc.main.id
  
  ingress {
    from_port   = 443
    to_port    = 443
    protocol   = "tcp"
    cidr_blocks = ["0.0.0.0/0"]
  }
  
  ingress {
    from_port   = 80
    to_port    = 80
    protocol   = "tcp"
    cidr_blocks = ["0.0.0.0/0"]
  }
  
  egress {
    from_port   = 0
    to_port    = 0
    protocol   = "-1"
    cidr_blocks = ["0.0.0.0/0"]
  }
  
  tags = {
    Name        = "tigerscan-alb-sg"
    Environment = var.environment
  }
}

# API Security Group
resource "aws_security_group" "api" {
  name        = "tigerscan-api-sg-${var.environment}"
  description = "Security group for TigerScan API"
  vpc_id     = aws_vpc.main.id
  
  ingress {
    from_port   = 8080
    to_port    = 8080
    protocol   = "tcp"
    security_groups = [aws_security_group.alb.id]
  }
  
  egress {
    from_port   = 0
    to_port    = 0
    protocol   = "-1"
    cidr_blocks = ["0.0.0.0/0"]
  }
  
  tags = {
    Name        = "tigerscan-api-sg"
    Environment = var.environment
  }
}

# Database Security Group
resource "aws_security_group" "database" {
  name        = "tigerscan-db-sg-${var.environment}"
  description = "Security group for TigerScan database"
  vpc_id     = aws_vpc.main.id
  
  ingress {
    from_port   = 5432
    to_port    = 5432
    protocol   = "tcp"
    security_groups = [aws_security_group.api.id]
  }
  
  tags = {
    Name        = "tigerscan-db-sg"
    Environment = var.environment
  }
}

# Redis Security Group
resource "aws_security_group" "redis" {
  name        = "tigerscan-redis-sg-${var.environment}"
  description = "Security group for TigerScan Redis"
  vpc_id     = aws_vpc.main.id
  
  ingress {
    from_port   = 6379
    to_port    = 6379
    protocol   = "tcp"
    security_groups = [aws_security_group.api.id]
  }
  
  tags = {
    Name        = "tigerscan-redis-sg"
    Environment = var.environment
  }
}

# Application Load Balancer
resource "aws_lb" "main" {
  name               = "tigerscan-alb-${var.environment}"
  internal           = false
  load_balancer_type = "application"
  security_groups   = [aws_security_group.alb.id]
  subnets           = aws_subnet.public[*].id
  
  enable_deletion_protection = var.environment == "production"
  
  tags = {
    Name        = "tigerscan-alb"
    Environment = var.environment
  }
}

# Target Group
resource "aws_lb_target_group" "api" {
  name     = "tigerscan-api-tg-${var.environment}"
  port     = 8080
  protocol = "HTTP"
  vpc_id   = aws_vpc.main.id
  
  health_check {
    enabled             = true
    healthy_threshold   = 2
    unhealthy_threshold = 5
    timeout            = 5
    interval           = 30
    path               = "/health"
    matcher            = "200"
  }
  
  tags = {
    Name        = "tigerscan-api-tg"
    Environment = var.environment
  }
}

# Listener
resource "aws_lb_listener" "main" {
  load_balancer_arn = aws_lb.main.arn
  port           = "443"
  protocol      = "HTTPS"
  
  ssl_policy   = "ELBSecurityPolicy-2016-08"
  certificate_arn = var.acm_certificate_arn
  
  default_action {
    type             = "forward"
    target_group_arn = aws_lb_target_group.api.arn
  }
}

# Listener HTTP redirect
resource "aws_lb_listener" "http" {
  load_balancer_arn = aws_lb.main.arn
  port             = "80"
  protocol         = "HTTP"
  
  default_action {
    type = "redirect"
    redirect {
      port        = "443"
      protocol   = "HTTPS"
      status_code = "HTTP_301"
    }
  }
}

# ECS Cluster
resource "aws_ecs_cluster" "main" {
  name = "tigerscan-cluster-${var.environment}"
  
  setting {
    name  = "containerInsights"
    value = "enabled"
  }
  
  tags = {
    Name        = "tigerscan-cluster"
    Environment = var.environment
  }
}

# ECS Task Definition - API
resource "aws_ecs_task_definition" "api" {
  family                   = "tigerscan-api-${var.environment}"
  network_mode             = "awsvpc"
  requires_compatibilities = ["FARGATE"]
  cpu                     = "1024"
  memory                  = "2048"
  
  container_definitions = jsonencode([
    {
      name      = "tigerscan-api"
      image    = "${var.api_image}"
      essential = true
      portMappings = [
        {
          containerPort = 8080
          protocol    = "tcp"
        }
      ]
      environment = [
        { name = "DB_URL", value = "postgres://${aws_db_instance.main.endpoint}" },
        { name = "REDIS_URL", value = "redis://${aws_elasticache_cluster.main.cache_nodes[0].address}" },
        { name = "ENVIRONMENT", value = var.environment }
      ]
      secrets = [
        { name = "API_KEY", valueFrom = aws_secretsmanager_secret.api_key.arn }
      ]
      logConfiguration = {
        logDriver = "awslogs"
        options = {
          "awslogs-group"         = "/ecs/tigerscan-api-${var.environment}"
          "awslogs-stream-prefix": "ecs"
          "awslogs-region"       = var.aws_region
        }
      }
    }
  ])
  
  tags = {
    Name        = "tigerscan-api-task"
    Environment = var.environment
  }
}

# ECS Service - API
resource "aws_ecs_service" "api" {
  name            = "tigerscan-api-${var.environment}"
  cluster         = aws_ecs_cluster.main.id
  task_definition = aws_ecs_task_definition.api.arn
  desired_count  = var.api_desired_count
  launch_type   = "FARGATE"
  
  network_configuration {
    subnets          = aws_subnet.private[*].id
    security_groups = [aws_security_group.api.id]
  }
  
  load_balancer {
    target_group_arn = aws_lb_target_group.api.arn
    container_name = "tigerscan-api"
    container_port = 8080
  }
  
  deployment_configuration {
    minimum_healthy_percent = 50
    maximum_percent        = 200
  }
  
  tags = {
    Name        = "tigerscan-api-service"
    Environment = var.environment
  }
}

# Auto Scaling Policy
resource "aws_appautoscaling_target" "api" {
  max_capacity = var.api_max_count
  min_capacity = var.api_min_count
  resource_id = "service/${aws_ecs_cluster.main.name}/${aws_ecs_service.api.name}"
  role_arn    = aws_iam_role.ecs_auto_scaling.arn
  scalable_dimension = "ecs:service:DesiredCount"
}

resource "aws_appautoscaling_policy" "api_scale_up" {
  name               = "tigerscan-api-scale-up-${var.environment}"
  policy_type       = "TargetTrackingScaling"
  resource_id      = aws_appautoscaling_target.api.resource_id
  scaling_target_id = aws_appautoscaling_target.api.id
  
  target_tracking_scaling_policy_configuration {
    predefined_metric_specification {
      metric_type = "ASGAverageCPUUtilization"
    }
    target_value = 70
  }
}

# Secrets Manager
resource "aws_secretsmanager_secret" "api_key" {
  name        = "tigerscan-api-key-${var.environment}"
  description = "TigerScan API Key"
  
  recovery_window_in_days = 7
  
  tags = {
    Name        = "tigerscan-api-key"
    Environment = var.environment
  }
}

# CloudWatch Log Group
resource "aws_cloudwatch_log_group" "api" {
  name              = "/ecs/tigerscan-api-${var.environment}"
  retention_in_days = 7
  
  tags = {
    Name        = "tigerscan-api-logs"
    Environment = var.environment
  }
}

# IAM Role for ECS Auto Scaling
resource "aws_iam_role" "ecs_auto_scaling" {
  name = "tigerscan-ecs-auto-scaling-${var.environment}"
  
  assume_role_policy = jsonencode({
    Version = "2012-10-17"
    Statement = [{
      Action = "sts:AssumeRole"
      Effect = "Allow"
      Principal = {
        Service = "application-autoscaling.amazonaws.com"
      }
    }]
  })
}

# Output
output "vpc_id" {
  value = aws_vpc.main.id
}

output "api_endpoint" {
  value = aws_lb.main.dns_name
}

output "database_endpoint" {
  value = aws_db_instance.main.endpoint
}

output "redis_endpoint" {
  value = aws_elasticache_cluster.main.cache_nodes[0].address
}