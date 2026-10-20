import React, { useRef } from 'react';
import { ContractFunction } from '../lib/sorobantypes';

interface FunctionSidebarProps {
    functions: ContractFunction[];
    selectedFunction: ContractFunction;
    onSelect: (func: ContractFunction) => void;
}

export const FunctionSidebar: React.FC<FunctionSidebarProps> = ({
    functions,
    selectedFunction,
    onSelect,
}) => {
    const listRef = useRef<HTMLDivElement>(null);

    const handleKeyDown = (e: React.KeyboardEvent<HTMLButtonElement>, idx: number) => {
        let nextIdx: number | null = null;
        if (e.key === 'ArrowDown') {
            e.preventDefault();
            nextIdx = (idx + 1) % functions.length;
        } else if (e.key === 'ArrowUp') {
            e.preventDefault();
            nextIdx = (idx - 1 + functions.length) % functions.length;
        } else if (e.key === 'Home') {
            e.preventDefault();
            nextIdx = 0;
        } else if (e.key === 'End') {
            e.preventDefault();
            nextIdx = functions.length - 1;
        } else if (e.key === 'Enter' || e.key === ' ') {
            e.preventDefault();
            onSelect(functions[idx]);
            return;
        }
        if (nextIdx !== null) {
            onSelect(functions[nextIdx]);
            const buttons = listRef.current?.querySelectorAll<HTMLButtonElement>('button[data-fn-idx]');
            buttons?.[nextIdx]?.focus();
        }
    };

    return (
        <div
            style={{
                backgroundColor: 'var(--bg-card)',
                borderRadius: '8px',
                padding: '24px',
                marginBottom: '24px',
                border: '1px solid #30363d',
            }}
        >
            <h2
                style={{
                    margin: '0 0 16px 0',
                    fontSize: '18px',
                    fontWeight: '600',
                    color: '#58a6ff',
                }}
            >
                Available Functions
            </h2>
            <div
                ref={listRef}
                role="listbox"
                aria-label="Contract functions"
                style={{ display: 'flex', flexDirection: 'column', gap: '8px' }}
            >
                {functions.map((func, idx) => {
                    const isSelected = selectedFunction.name === func.name;
                    return (
                        <button
                            key={func.name}
                            role="option"
                            aria-selected={isSelected}
                            data-fn-idx={idx}
                            tabIndex={isSelected ? 0 : -1}
                            onClick={() => onSelect(func)}
                            onKeyDown={(e) => handleKeyDown(e, idx)}
                            style={{
                                padding: '12px 16px',
                                backgroundColor: isSelected ? '#8957e5' : '#21262d',
                                color: isSelected ? '#fff' : '#c9d1d9',
                                border: isSelected ? '1px solid #8957e5' : '1px solid #30363d',
                                borderRadius: '6px',
                                textAlign: 'left',
                                cursor: 'pointer',
                                fontWeight: isSelected ? '600' : '500',
                                transition: 'all 0.2s',
                                fontSize: '14px',
                                display: 'flex',
                                justifyContent: 'space-between',
                                alignItems: 'center',
                            }}
                            onMouseEnter={(e) => {
                                if (!isSelected) {
                                    e.currentTarget.style.backgroundColor = '#21262d';
                                    e.currentTarget.style.borderColor = '#8957e5';
                                }
                            }}
                            onMouseLeave={(e) => {
                                if (!isSelected) {
                                    e.currentTarget.style.backgroundColor = '#21262d';
                                    e.currentTarget.style.borderColor = '#30363d';
                                }
                            }}
                        >
                            <span>{func.name}</span>
                            <span style={{ fontSize: '12px', opacity: '0.7' }}>
                                {func.inputs.length} args
                            </span>
                        </button>
                    );
                })}
            </div>
        </div>
    );
};
