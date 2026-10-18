# Fee Comparison Feature

## Overview

The Fee Comparison feature allows users to compare transaction fees between Testnet and Mainnet environments in the Sky Moon Scope application. This helps developers understand the cost differences before deploying to production.

## Implementation

### Backend (stellarRpc.ts)

The fee comparison logic is implemented in `web/lib/stellarRpc.ts`:

- **`fetchFeeComparison()`**: Fetches fee stats from both Testnet and Mainnet RPC endpoints simultaneously using `Promise.allSettled()`
- **Percentage Difference Calculation**: Computes the percentage difference between Mainnet and Testnet fees
- **Error Handling**: Gracefully handles RPC failures by falling back to default fee estimates

### Frontend (FeeEstimationPreview.tsx)

The UI component in `web/components/FeeEstimationPreview.tsx`:

- **Toggle Button**: "Compare Networks" button to show/hide the comparison view
- **Side-by-side Cards**: Displays fee estimates for both Testnet and Mainnet
- **Visual Indicators**: 
  - Arrow up/down icons showing which network has higher fees
  - Color-coded cards (cyan for Testnet, emerald for Mainnet)
  - Percentage difference badge
- **Detailed Breakdown**: Shows total fees, low/high bump options, and surge multipliers

### Data Types

Added to `web/lib/sorobantypes.ts`:

```typescript
export interface NetworkFeeComparison {
  testnet: FeeEstimate;
  mainnet: FeeEstimate;
  percentageDifference: number;
  testnetHigher: boolean;
}
```

## Usage

1. Run a simulation to generate fee estimates
2. Click the "Compare Networks" button in the Fee Estimation section
3. View the side-by-side comparison of Testnet vs Mainnet fees
4. See the percentage difference and detailed breakdown

## Testing

Unit tests are in `web/lib/stellarRpc.feeComparison.test.cjs`:

- Tests for correct percentage difference calculation
- Tests for parallel RPC calls
- Tests for RPC failure handling
- Tests for fee bump options between networks
- Tests for surge multiplier preservation

Run tests with:
```bash
npm test
```

## Example Output

```
Mainnet fees are 0.98% higher

Testnet         Mainnet
─────────────  ─────────────
Total Fee      Total Fee
0.00051 XLM    0.00052 XLM
Low Bump       Low Bump
0.00051 XLM    0.00077 XLM
High Bump      High Bump
0.00102 XLM    0.00154 XLM

Testnet Surge: 1x
Mainnet Surge: 1.2x
```

## Benefits

- **Cost Awareness**: Helps developers understand fee differences before mainnet deployment
- **Informed Decisions**: Enables better cost optimization strategies
- **Risk Mitigation**: Reduces surprise costs when moving from testnet to mainnet
- **Transparency**: Clear visual representation of fee structures

## Future Enhancements

- Add historical fee trend charts
- Include more networks (Futurenet, custom RPCs)
- Export comparison data as CSV
- Add fee optimization suggestions based on comparison
