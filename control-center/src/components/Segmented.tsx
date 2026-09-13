interface SegmentedOption<T extends string> {
  value: T;
  label: string;
}

interface SegmentedProps<T extends string> {
  label: string;
  options: ReadonlyArray<SegmentedOption<T>>;
  value: T;
  onChange: (value: T) => void;
  compact?: boolean;
}

export function Segmented<T extends string>({ label, options, value, onChange, compact }: SegmentedProps<T>) {
  return (
    <div aria-label={label} className="segmented" data-compact={compact} role="group">
      {options.map((option) => (
        <button aria-pressed={option.value === value} className="segmented-option button secondary" key={option.value} onClick={() => onChange(option.value)} type="button">{option.label}</button>
      ))}
    </div>
  );
}
