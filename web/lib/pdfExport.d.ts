import type { NutritionMetric } from './nutritionLabel';

export interface PdfExportOptions {
  functionName?: string;
  costStroops?: number;
  generatedAt?: string;
}

/**
 * Assemble a minimal PDF 1.4 document containing the nutrition label data
 * and return it as a Uint8Array.
 *
 * Uses standard Helvetica/Helvetica-Bold Type1 fonts so no font embedding
 * is required and the result opens in any PDF viewer.
 */
export function buildPdfBytes(metrics: NutritionMetric[], options?: PdfExportOptions): Uint8Array;

/**
 * Trigger a browser download of the generated PDF bytes.
 * No-ops in non-browser environments (SSR / Node test).
 */
export function downloadPdf(pdfBytes: Uint8Array, filename?: string): void;

/**
 * Build the raw PDF content stream string (exposed for unit testing).
 */
export function buildContentStream(
  metrics: NutritionMetric[],
  options?: PdfExportOptions,
): string;

/**
 * Compact number formatter used inside the PDF stream
 * (mirrors nutritionLabel.js `formatValue`).
 */
export function formatCompact(value: number): string;
