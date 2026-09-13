export const splitSubstitution = (mapping: string) => {
  const separator = mapping.indexOf("=");
  return separator < 0
    ? { source: mapping, replacement: mapping }
    : {
        source: mapping.slice(0, separator).trim(),
        replacement: mapping.slice(separator + 1).trim(),
      };
};
