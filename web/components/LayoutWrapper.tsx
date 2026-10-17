/**
 * LayoutWrapper component to prevent layout shift on responsive changes
 * Ensures consistent width and padding across all screen sizes
 */

interface LayoutWrapperProps {
  children: React.ReactNode;
  className?: string;
}

export function LayoutWrapper({ children, className = '' }: LayoutWrapperProps) {
  return (
    <div
      className={`
        mx-auto w-full max-w-7xl px-4 sm:px-6 lg:px-8
        /* Prevent layout shift by maintaining consistent width */
        /* box-sizing is inherited from globals.css */
        ${className}
      `}
      style={{
        /* Explicitly set box-sizing for safety */
        boxSizing: 'border-box',
      }}
    >
      {children}
    </div>
  );
}

/**
 * ResponsiveGrid component for two-column layouts that don't shift on sidebar collapse
 * Maintains consistent grid structure with proper spacing
 */
interface ResponsiveGridProps {
  left: React.ReactNode;
  right: React.ReactNode;
  className?: string;
}

export function ResponsiveGrid({ left, right, className = '' }: ResponsiveGridProps) {
  return (
    <div
      className={`
        grid w-full grid-cols-1 gap-6 lg:grid-cols-2
        /* Ensure grid maintains width during layout changes */
        ${className}
      `}
      style={{
        boxSizing: 'border-box',
        /* Prevent grid items from shifting */
        minWidth: 0,
      }}
    >
      <div style={{ boxSizing: 'border-box', minWidth: 0 }}>
        {left}
      </div>
      <div style={{ boxSizing: 'border-box', minWidth: 0 }}>
        {right}
      </div>
    </div>
  );
}
