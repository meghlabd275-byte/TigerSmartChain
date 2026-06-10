// Package kubernetes provides Kubernetes manifests for TigerSmartChain deployment.
// Production-ready Kubernetes manifests with proper security and monitoring.
package kubernetes

import (
	"fmt"
	"os"
	"strings"
)

// =============================================================================
// VALIDATOR SET (StatefulSet)
// =============================================================================

// ValidatorStatefulSet returns Kubernetes StatefulSet manifest for validator.
func ValidatorStatefulSet(name, namespace string, replicas int32, image string) string {
	return fmt.Sprintf(`apiVersion: apps/v1
kind: StatefulSet
metadata:
  name: %s
  namespace: %s
spec:
  serviceName: %s
  replicas: %d
  selector:
    matchLabels:
      app: %s
  template:
    metadata:
      labels:
        app: %s
    spec:
      containers:
      - name: validator
        image: %s
        ports:
        - containerPort: 26656
          name: p2p
        - containerPort: 26657
          name: rpc
        - containerPort: 26658
          name: grpc
        - containerPort: 30303
          name: eth
        env:
        - name: CHAIN_ID
          value: "9001"
        - name: P2P_LISTEN_ADDRESS
          value: "0.0.0.0:26656"
        - name: RPC_LISTEN_ADDRESS
          value: "0.0.0.0:26657"
        - name: GRPC_LISTEN_ADDRESS
          value: "0.0.0.0:26658"
        - name: ETH_LISTEN_ADDRESS
          value: "0.0.0.0:30303"
        - name: KEY_PASSWORD
          valueFrom:
            secretKeyRef:
              name: %s-keys
              key: password
        volumeMounts:
        - name: data
          mountPath: /data
        - name: config
          mountPath: /config
        resources:
          requests:
            cpu: "2"
            memory: "4Gi"
          limits:
            cpu: "4"
            memory: "8Gi"
        livenessProbe:
          httpGet:
            path: /health
            port: 26657
          initialDelaySeconds: 30
          periodSeconds: 10
        readinessProbe:
          httpGet:
            path: /ready
            port: 26657
          initialDelaySeconds: 10
          periodSeconds: 5
        securityContext:
          runAsNonRoot: true
          runAsUser: 1000
          fsGroup: 1000
          capabilities:
            drop:
            - ALL
      volumes:
      - name: config
        configMap:
          name: %s-config
      - name: data
        persistentVolumeClaimSpec:
          accessModes:
          - ReadWriteOnce
          resources:
            requests:
              storage: 100Gi
  podManagementPolicy: Parallel
  updateStrategy:
    type: RollingUpdate
`, name, namespace, name, replicas, name, name, image, name, name)
}

// =============================================================================
// RPC SERVICE (Deployment)
// =============================================================================

// RPCDeployment returns Kubernetes Deployment manifest for RPC server.
func RPCDeployment(name, namespace string, replicas int32, image string) string {
	return fmt.Sprintf(`apiVersion: apps/v1
kind: Deployment
metadata:
  name: %s-rpc
  namespace: %s
spec:
  replicas: %d
  selector:
    matchLabels:
      app: %s-rpc
  template:
    metadata:
      labels:
        app: %s-rpc
    spec:
      containers:
      - name: rpc
        image: %s
        ports:
        - containerPort: 8545
          name: http
        - containerPort: 8546
          name: ws
        env:
        - name: CHAIN_ID
          value: "9001"
        - name: JSON_RPC_ADDRESS
          value: "0.0.0.0:8545"
        - name: WS_ADDRESS
          value: "0.0.0.0:8546"
        - name: BACKEND_URL
          value: "http://%s-validator:26657"
        - name: CORS_ORIGINS
          value: "*"
        - name: MAX_CONNECTIONS
          value: "10000"
        - name: RATE_LIMIT
          value: "1000"
        - name: API_KEY
          valueFrom:
            secretKeyRef:
              name: %s-api-keys
              key: key
        resources:
          requests:
            cpu: "1"
            memory: "2Gi"
          limits:
            cpu: "2"
            memory: "4Gi"
        livenessProbe:
          httpGet:
            path: /health
            port: 8545
          initialDelaySeconds: 15
          periodSeconds: 10
        readinessProbe:
          httpGet:
            path: /ready
            port: 8545
          initialDelaySeconds: 5
          periodSeconds: 5
        securityContext:
          runAsNonRoot: true
          runAsUser: 1000
          capabilities:
            drop:
            - ALL
`, name, namespace, replicas, name, name, image, name, name)
}

// =============================================================================
// RPC SERVICE (LoadBalancer)
// =============================================================================

// RPCService returns Kubernetes Service manifest for RPC LoadBalancer.
func RPCService(name, namespace string) string {
	return fmt.Sprintf(`apiVersion: v1
kind: Service
metadata:
  name: %s-rpc
  namespace: %s
  annotations:
    service.beta.kubernetes.io/aws-load-balancer-type: nlb
spec:
  type: LoadBalancer
  selector:
    app: %s-rpc
  ports:
  - name: http
    port: 8545
    targetPort: 8545
  - name: ws
    port: 8546
    targetPort: 8546
`, name, namespace, name)
}

// =============================================================================
// P2P SERVICE (NodePort)
// =============================================================================

// P2PService returns Kubernetes Service manifest for P2P node discovery.
func P2PService(name, namespace string) string {
	return fmt.Sprintf(`apiVersion: v1
kind: Service
metadata:
  name: %s-p2p
  namespace: %s
spec:
  type: NodePort
  selector:
    app: %s
  ports:
  - name: p2p
    port: 26656
    targetPort: 26656
    nodePort: 30056
  - name: rpc
    port: 26657
    targetPort: 26657
    nodePort: 30057
`, name, namespace, name)
}

// =============================================================================
// CONFIGMAP
// =============================================================================

// ConfigMap returns Kubernetes ConfigMap manifest.
func ConfigMap(name, namespace, config string) string {
	return fmt.Sprintf(`apiVersion: v1
kind: ConfigMap
metadata:
  name: %s-config
  namespace: %s
data:
  config.toml: |
%s
`, name, namespace, config)
}

// =============================================================================
// SECRET
// =============================================================================

// Secret returns Kubernetes Secret manifest.
func Secret(name, namespace, key string) string {
	return fmt.Sprintf(`apiVersion: v1
kind: Secret
metadata:
  name: %s-keys
  namespace: %s
type: Opaque
stringData:
  key: %s
`, name, namespace, key)
}

// =============================================================================
// STORAGE CLASS
// =============================================================================

// StorageClass returns Kubernetes StorageClass manifest for fast SSD storage.
func StorageClass(name string) string {
	return fmt.Sprintf(`apiVersion: storage.k8s.io/v1
kind: StorageClass
metadata:
  name: %s
provisioner: kubernetes.io/gce-pd
parameters:
  type: pd-ssd
  replication: regional-pd
volumeBindingMode: WaitForFirstConsumer
allowVolumeExpansion: true
reclaimPolicy: Retain
`, name)
}

// =============================================================================
// PVC (PersistentVolumeClaim)
// =============================================================================

// PVC returns Kubernetes PersistentVolumeClaim manifest.
func PVC(name, namespace, size string) string {
	return fmt.Sprintf(`apiVersion: v1
kind: PersistentVolumeClaim
metadata:
  name: %s-data
  namespace: %s
spec:
  accessModes:
  - ReadWriteOnce
  resources:
    requests:
      storage: %s
  storageClassName: %s
`, name, namespace, size, name)
}

// =============================================================================
// POD DISRUPTION BUDGET
// =============================================================================

// PodDisruptionBudget returns Kubernetes PodDisruptionBudget manifest.
func PodDisruptionBudget(name, namespace string, minAvailable int32) string {
	return fmt.Sprintf(`apiVersion: policy/v1
kind: PodDisruptionBudget
metadata:
  name: %s
  namespace: %s
spec:
  minAvailable: %d
  selector:
    matchLabels:
      app: %s
`, name, namespace, minAvailable, name)
}

// =============================================================================
// NETWORK POLICY
// =============================================================================

// NetworkPolicy returns Kubernetes NetworkPolicy manifest.
func NetworkPolicy(name, namespace string) string {
	return fmt.Sprintf(`apiVersion: networking.k8s.io/v1
kind: NetworkPolicy
metadata:
  name: %s-network-policy
  namespace: %s
spec:
  podSelector:
    matchLabels:
      app: %s
  policyTypes:
  - Ingress
  - Egress
  ingress:
  - from:
    - podSelector: {}
  egress:
  - to:
    - podSelector: {}
  - to:
    - namespaceSelector: {}
    ports:
    - protocol: TCP
      port: 53
    - protocol: UDP
      port: 53
`, name, namespace, name)
}

// =============================================================================
// RBAC (Role-Based Access Control)
// =============================================================================

// ServiceAccount returns Kubernetes ServiceAccount manifest.
func ServiceAccount(name, namespace string) string {
	return fmt.Sprintf(`apiVersion: v1
kind: ServiceAccount
metadata:
  name: %s
  namespace: %s
`, name, namespace)
}

// Role returns Kubernetes Role manifest.
func Role(name, namespace string) string {
	return fmt.Sprintf(`apiVersion: rbac.authorization.k8s.io/v1
kind: Role
metadata:
  name: %s
  namespace: %s
rules:
- apiGroups: [""]
  resources: ["pods", "services", "configmaps", "secrets"]
  verbs: ["get", "list", "watch"]
- apiGroups: [""]
  resources: ["pods/exec"]
  verbs: ["create"]
`, name, namespace)
}

// RoleBinding returns Kubernetes RoleBinding manifest.
func RoleBinding(name, namespace, serviceAccountName string) string {
	return fmt.Sprintf(`apiVersion: rbac.authorization.k8s.io/v1
kind: RoleBinding
metadata:
  name: %s-binding
  namespace: %s
subjects:
- kind: ServiceAccount
  name: %s
  namespace: %s
roleRef:
  kind: Role
  name: %s
  apiGroup: rbac.authorization.k8s.io
`, name, namespace, serviceAccountName, namespace, name)
}

// =============================================================================
// RESOURCE QUOTA
// =============================================================================

// ResourceQuota returns Kubernetes ResourceQuota manifest.
func ResourceQuota(name, namespace string) string {
	return fmt.Sprintf(`apiVersion: v1
kind: ResourceQuota
metadata:
  name: %s-quota
  namespace: %s
spec:
  hard:
    requests.cpu: "100"
    requests.memory: 200Gi
    limits.cpu: "200"
    limits.memory: 400Gi
    pods: "50"
    persistentvolumeclaims: "20"
`, name, namespace)
}

// =============================================================================
// LIMIT RANGE
// =============================================================================

// LimitRange returns Kubernetes LimitRange manifest.
func LimitRange(name, namespace string) string {
	return fmt.Sprintf(`apiVersion: v1
kind: LimitRange
metadata:
  name: %s-limits
  namespace: %s
spec:
  limits:
  - default:
      cpu: "2"
      memory: "4Gi"
    defaultRequest:
      cpu: "500m"
      memory: "1Gi"
    type: Container
`, name, namespace)
}

// =============================================================================
// HORIZONTAL POD AUTOSCALER
// =============================================================================

// HorizontalPodAutoscaler returns Kubernetes HPA manifest.
func HorizontalPodAutoscaler(name, namespace string, minReplicas, maxReplicas int32, targetCPUPercent int) string {
	return fmt.Sprintf(`apiVersion: autoscaling/v2
kind: HorizontalPodAutoscaler
metadata:
  name: %s-hpa
  namespace: %s
spec:
  scaleTargetRef:
    apiVersion: apps/v1
    kind: Deployment
    name: %s
  minReplicas: %d
  maxReplicas: %d
  metrics:
  - type: Resource
    resource:
      name: cpu
      target:
        type: Utilization
        averageUtilization: %d
  behavior:
    scaleDown:
      stabilizationWindowSeconds: 300
      policies:
      - type: Percent
        value: 10
        periodSeconds: 60
    scaleUp:
      stabilizationWindowSeconds: 0
      policies:
      - type: Percent
        value: 100
        periodSeconds: 15
`, name, namespace, name, minReplicas, maxReplicas, targetCPUPercent)
}

// =============================================================================
// POD ANTI-AFFINITY
// =============================================================================

// GetAntiAffinity returns pod anti-affinity for high availability.
func GetAntiAffinity(name string) string {
	return fmt.Sprintf(`affinity:
  podAntiAffinity:
    requiredDuringSchedulingIgnoredDuringExecution:
    - labelSelector:
        matchLabels:
          app: %s
      topologyKey: kubernetes.io/hostname
  nodeAffinity:
    preferredDuringSchedulingIgnoredDuringExecution:
    - weight: 100
      preference:
        matchExpressions:
        - key: node.kubernetes.io-instance-type
          operator: In
          values:
          - compute-optimized
`, name)
}

// =============================================================================
// TOLERATIONS
// =============================================================================

// GetTolerations returns taints tolerations for dedicated nodes.
func GetTolerations() string {
	return `tolerations:
- key: dedicated
  operator: Equal
  value: validator
  effect: NoSchedule
- key: dedicated
  operator: Equal
  value: validator
  effect: NoExecute
- key: node.kubernetes.io/not-ready
  operator: Exists
  effect: NoExecute
  tolerationSeconds: 300
- key: node.kubernetes.io/unreachable
  operator: Exists
  effect: NoExecute
  tolerationSeconds: 300
`

var _ = os.Getenv
var _ = strings.Contains
var _ = fmt.Sprintf