'use client';

import React, { useMemo, useEffect, useState } from 'react';
import { motion } from 'framer-motion';
import { Cpu } from 'lucide-react';

interface CPUGaugeProps {
  /** Current CPU instruction utilization (0-100) */
  utilization: number;
  /** Maximum CPU instructions (for reference/scaling) */
  maxInstructions?: number;
  /** Current CPU instructions used */
  currentInstructions?: number;
  /** Optional title */
  title?: string;
  /** Optional className for container */
  className?: string;
  /** Animate on mount */
  animate?: boolean;
}

type SeverityLevel = 'low' | 'moderate' | 'high' | 'critical';

const getSeverityLevel = (utilization: number): SeverityLevel => {
  if (utilization >= 85) return 'critical';
  if (utilization >= 70) return 'high';
  if (utilization >= 50) return 'moderate';
  return 'low';
};

const getSeverityColor = (level: SeverityLevel): { needle: string; zone: string; label: string } => {
  const colors: Record<SeverityLevel, { needle: string; zone: string; label: string }> = {
    low: { needle: '#10b981', zone: 'from-emerald-500/20 to-emerald-600/20', label: 'Well within budget' },
    moderate: { needle: '#f59e0b', zone: 'from-amber-500/20 to-amber-600/20', label: 'Moderate usage' },
    high: { needle: '#ef7f35', zone: 'from-orange-500/20 to-orange-600/20', label: 'Approaching limit' },
    critical: { needle: '#ef4444', zone: 'from-red-500/20 to-red-600/20', label: 'Critical usage' },
  };
  return colors[level];
};

export const CPUGauge: React.FC<CPUGaugeProps> = ({
  utilization,
  maxInstructions,
  currentInstructions,
  title = 'CPU Instruction Utilization',
  className = '',
  animate = true,
}) => {
  const [isVisible, setIsVisible] = useState(false);

  useEffect(() => {
    setIsVisible(true);
  }, []);

  // Clamp utilization between 0 and 100
  const clampedUtil = Math.max(0, Math.min(100, utilization));
  const severityLevel = getSeverityLevel(clampedUtil);
  const colors = getSeverityColor(severityLevel);

  // Calculate needle rotation (180 degree range: -90 to 90 degrees)
  const needleRotation = (clampedUtil / 100) * 180 - 90;

  // Formatted display values
  const utilPercent = clampedUtil.toFixed(1);
  const formattedCurrent = currentInstructions
    ? new Intl.NumberFormat('en-US', { notation: 'compact', compactDisplay: 'short' }).format(currentInstructions)
    : null;
  const formattedMax = maxInstructions
    ? new Intl.NumberFormat('en-US', { notation: 'compact', compactDisplay: 'short' }).format(maxInstructions)
    : null;

  return (
    <div className={`flex flex-col items-center gap-4 ${className}`}>
      {/* Header */}
      <div className="flex items-center gap-2 w-full">
        <Cpu className="text-cyan-400" size={20} />
        <h3 className="text-lg font-semibold text-gray-200">{title}</h3>
      </div>

      {/* Gauge Container */}
      <div className="relative w-full max-w-sm aspect-square flex items-center justify-center">
        {/* Outer circular background */}
        <motion.div
          className="absolute inset-0 rounded-full border-8 border-gray-700/50 bg-gradient-to-b from-gray-900/50 to-gray-950/70 shadow-lg"
          initial={animate ? { opacity: 0, scale: 0.9 } : { opacity: 1, scale: 1 }}
          animate={isVisible ? { opacity: 1, scale: 1 } : { opacity: 1, scale: 1 }}
          transition={{ duration: 0.6, ease: 'easeOut' }}
        />

        {/* Color zones (severity bands) */}
        <svg className="absolute inset-0 w-full h-full" viewBox="0 0 200 200">
          {/* Low zone (0-50%) */}
          <path
            d="M 100,100 L 160,60 A 60,60 0 0,1 160,140 Z"
            fill="url(#gradientLow)"
            fillOpacity="0.15"
          />
          <defs>
            <linearGradient id="gradientLow" x1="0%" y1="0%" x2="0%" y2="100%">
              <stop offset="0%" stopColor="#10b981" stopOpacity="0.3" />
              <stop offset="100%" stopColor="#10b981" stopOpacity="0.1" />
            </linearGradient>
            <linearGradient id="gradientModerate" x1="0%" y1="0%" x2="0%" y2="100%">
              <stop offset="0%" stopColor="#f59e0b" stopOpacity="0.3" />
              <stop offset="100%" stopColor="#f59e0b" stopOpacity="0.1" />
            </linearGradient>
            <linearGradient id="gradientHigh" x1="0%" y1="0%" x2="0%" y2="100%">
              <stop offset="0%" stopColor="#ef7f35" stopOpacity="0.3" />
              <stop offset="100%" stopColor="#ef7f35" stopOpacity="0.1" />
            </linearGradient>
            <linearGradient id="gradientCritical" x1="0%" y1="0%" x2="0%" y2="100%">
              <stop offset="0%" stopColor="#ef4444" stopOpacity="0.3" />
              <stop offset="100%" stopColor="#ef4444" stopOpacity="0.1" />
            </linearGradient>
          </defs>

          {/* Moderate zone (50-70%) */}
          <path
            d="M 100,100 L 160,140 A 60,60 0 0,1 60,140 Z"
            fill="url(#gradientModerate)"
            fillOpacity="0.15"
          />

          {/* High zone (70-85%) */}
          <path
            d="M 100,100 L 60,140 A 60,60 0 0,1 40,100 Z"
            fill="url(#gradientHigh)"
            fillOpacity="0.15"
          />

          {/* Critical zone (85-100%) */}
          <path
            d="M 100,100 L 40,100 A 60,60 0 0,1 60,60 Z"
            fill="url(#gradientCritical)"
            fillOpacity="0.15"
          />

          {/* Scale tick marks and labels */}
          {[0, 25, 50, 75, 100].map((tick) => {
            const angle = (tick / 100) * 180 - 90;
            const rad = (angle * Math.PI) / 180;
            const outerR = 60;
            const innerR = 55;
            const x1 = 100 + outerR * Math.cos(rad);
            const y1 = 100 + outerR * Math.sin(rad);
            const x2 = 100 + innerR * Math.cos(rad);
            const y2 = 100 + innerR * Math.sin(rad);

            return (
              <g key={`tick-${tick}`}>
                <line x1={x1} y1={y1} x2={x2} y2={y2} stroke="#4b5563" strokeWidth="1" />
                <text
                  x={100 + (outerR + 8) * Math.cos(rad)}
                  y={100 + (outerR + 8) * Math.sin(rad)}
                  textAnchor="middle"
                  dominantBaseline="middle"
                  fill="#8b949e"
                  fontSize="10"
                  fontFamily="monospace"
                >
                  {tick}%
                </text>
              </g>
            );
          })}
        </svg>

        {/* Animated Needle */}
        <motion.div
          className="absolute inset-0 flex items-center justify-center"
          initial={animate ? { opacity: 0 } : { opacity: 1 }}
          animate={isVisible ? { opacity: 1 } : { opacity: 1 }}
          transition={{ delay: 0.3, duration: 0.6 }}
        >
          <svg className="absolute w-full h-full" viewBox="0 0 200 200" style={{ pointerEvents: 'none' }}>
            {/* Needle */}
            <motion.g
              style={{
                transformOrigin: '100px 100px',
                transform: 'rotate3d(0, 0, 1, 0deg)',
              }}
              animate={isVisible && animate ? { rotate: needleRotation } : { rotate: 0 }}
              transition={{ type: 'spring', damping: 20, stiffness: 100, mass: 1 }}
            >
              {/* Needle shadow for depth */}
              <line
                x1="100"
                y1="100"
                x2="100"
                y2="55"
                stroke="rgba(0, 0, 0, 0.3)"
                strokeWidth="4"
                strokeLinecap="round"
              />
              {/* Main needle */}
              <line
                x1="100"
                y1="100"
                x2="100"
                y2="55"
                stroke={colors.needle}
                strokeWidth="3"
                strokeLinecap="round"
                filter="drop-shadow(0 0 3px rgba(0, 0, 0, 0.5))"
              />
            </motion.g>

            {/* Center circle */}
            <circle cx="100" cy="100" r="6" fill={colors.needle} />
            <circle cx="100" cy="100" r="4" fill="#1a1a1a" />
          </svg>
        </motion.div>
      </div>

      {/* Metrics Display */}
      <div className="w-full max-w-sm">
        {/* Utilization percentage */}
        <div className="mb-3">
          <motion.div
            className="text-center"
            initial={animate ? { opacity: 0, y: 10 } : { opacity: 1, y: 0 }}
            animate={isVisible ? { opacity: 1, y: 0 } : { opacity: 1, y: 0 }}
            transition={{ delay: 0.4, duration: 0.5 }}
          >
            <div className="text-4xl font-bold" style={{ color: colors.needle }}>
              {utilPercent}%
            </div>
            <div className="text-xs text-gray-400 mt-1">{colors.label}</div>
          </motion.div>
        </div>

        {/* Severity indicator bar */}
        <div className="relative h-2 rounded-full bg-gray-800/50 overflow-hidden border border-gray-700/30">
          <motion.div
            className="h-full rounded-full transition-colors duration-300"
            style={{ backgroundColor: colors.needle }}
            initial={animate ? { width: '0%' } : { width: `${clampedUtil}%` }}
            animate={isVisible ? { width: `${clampedUtil}%` } : { width: `${clampedUtil}%` }}
            transition={{ duration: 1, ease: 'easeOut' }}
          />
        </div>

        {/* Breakdown details */}
        {(currentInstructions || maxInstructions) && (
          <motion.div
            className="mt-4 p-3 bg-gray-800/30 rounded-lg border border-gray-700/30 text-xs text-gray-300 font-mono space-y-1"
            initial={animate ? { opacity: 0, y: 5 } : { opacity: 1, y: 0 }}
            animate={isVisible ? { opacity: 1, y: 0 } : { opacity: 1, y: 0 }}
            transition={{ delay: 0.5, duration: 0.5 }}
          >
            {currentInstructions && (
              <div className="flex justify-between">
                <span className="text-gray-500">Current:</span>
                <span className="text-gray-200">{formattedCurrent} instructions</span>
              </div>
            )}
            {maxInstructions && (
              <div className="flex justify-between">
                <span className="text-gray-500">Limit:</span>
                <span className="text-gray-200">{formattedMax} instructions</span>
              </div>
            )}
          </motion.div>
        )}
      </div>
    </div>
  );
};

export default CPUGauge;
