# Crossplane Composition Performance Investigation

## Context

We observed high latency in Crossplane compositions during cluster provisioning. The median composition reconciliation time increased from 2.3s to 18.7s after upgrading to Crossplane v1.14.

## Root Cause

The composition function pipeline was executing sequentially instead of in parallel. The `function-patch-and-transform` function was being called three times per composition, each with a full deep copy of the composite resource state.

## Solution

- Enabled parallel function execution via `spec.patchSets[*].parallel: true`
- Reduced unnecessary state copying by using references in patch pipelines
- Added composition function latency metrics to Prometheus

## Impact

Composition reconciliation time dropped to 3.1s (p50) and 5.8s (p99). No regressions observed in correctness or resource drift detection.

## References

- Crossplane issue #4821
- Internal incident post-mortem: INC-2024-089
