# Kubernetes HPA and VPA Tuning

## Overview

Tuned Horizontal Pod Autoscaler (HPA) and Vertical Pod Autoscaler (VPA) for the payment processing service to handle traffic spikes during peak hours.

## HPA Configuration

- Target metric: custom metric `http_requests_per_second` from Prometheus
- Min replicas: 3
- Max replicas: 50
- Target value: 1000 requests/sec per pod
- Scale-down stabilization: 300s

## VPA Configuration

- Mode: `Auto` for non-stateful workloads
- Resource policy: CPU requests between 100m and 4000m
- Memory requests between 256Mi and 8Gi

## Results

During Black Friday traffic (5x normal load), the system scaled from 3 to 28 replicas with no manual intervention. p99 latency stayed under 200ms. VPA reduced memory waste by 40% during off-peak hours.

## Lessons Learned

- VPA and HPA can conflict if both target CPU/memory — use custom metrics for HPA
- Scale-down stabilization is critical to prevent flapping
- Test autoscaling under synthetic load before real traffic events
