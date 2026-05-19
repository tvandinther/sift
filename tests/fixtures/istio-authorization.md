# Istio AuthorizationPolicy for Namespace Isolation

## Problem

Multi-tenant clusters need strict namespace isolation at the service mesh layer. Default Istio behavior allows cross-namespace traffic without explicit policy.

## Implementation

Created a baseline `AuthorizationPolicy` that denies all cross-namespace traffic by default:

```yaml
apiVersion: security.istio.io/v1
kind: AuthorizationPolicy
metadata:
  name: deny-cross-namespace
  namespace: istio-system
spec:
  action: DENY
  rules:
  - from:
    - source:
        notNamespaces: ["{{.PodNamespace}}"]
```

Then selectively allow cross-namespace communication via explicit allow policies in each namespace that requires it.

## Testing

Verified with `istioctl analyze` and live traffic tests between namespaces. Confirmed that unauthorized cross-namespace requests return HTTP 403.

## Rollout

Applied to production via GitOps (ArgoCD). No incidents during rollout. Monitoring with Kiali showed expected traffic patterns.
