interface SwitchControlProps {
  label: string;
  checked: boolean;
  disabled?: boolean;
  onChange: (checked: boolean) => void;
}

export function SwitchControl({ label, checked, disabled, onChange }: SwitchControlProps) {
  return <label className="checkbox-label"><input type="checkbox" checked={checked} disabled={disabled} onChange={(event) => onChange(event.target.checked)} /><span>{label}</span></label>;
}
