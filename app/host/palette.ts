// The avatar palette: the names a Bot may carry, and the app tokens they map to. Its values
// come from design/tokens.json: change them there.
// design:begin palette
const BRAND: Record<string, string> = {
  black: 'brand.black', '#000000': 'brand.black', 'brand.black': 'brand.black',
  white: 'brand.white', '#f2f2f0': 'brand.white', 'brand.white': 'brand.white',
  brown: 'brand.brown', '#936439': 'brand.brown', 'brand.brown': 'brand.brown',
  red: 'brand.red', '#ff263c': 'brand.red', 'brand.red': 'brand.red',
  orange: 'brand.orange', '#ff6700': 'brand.orange', 'brand.orange': 'brand.orange',
  amber: 'brand.amber', '#ffaf38': 'brand.amber', 'brand.amber': 'brand.amber',
  yellow: 'brand.yellow', '#ff9800': 'brand.yellow', 'brand.yellow': 'brand.yellow',
  green: 'brand.green', '#00c972': 'brand.green', 'brand.green': 'brand.green',
  teal: 'brand.default', '#1cc3b0': 'brand.default', 'brand.default': 'brand.default',
  cyan: 'brand.cyan', '#00bca6': 'brand.cyan', 'brand.cyan': 'brand.cyan',
  blue: 'brand.blue', '#1084fe': 'brand.blue', 'brand.blue': 'brand.blue',
  purple: 'brand.purple', '#a97efe': 'brand.purple', 'brand.purple': 'brand.purple',
  violet: 'brand.violet', '#9159fe': 'brand.violet', 'brand.violet': 'brand.violet',
  pink: 'brand.pink', '#ff5eb1': 'brand.pink', 'brand.pink': 'brand.pink',
  magenta: 'brand.magenta', '#ff309b': 'brand.magenta', 'brand.magenta': 'brand.magenta',
  gray: 'brand.gray', '#777777': 'brand.gray', 'brand.gray': 'brand.gray',
};
export const SWATCHES = ['brand.brown', 'brand.red', 'brand.orange', 'brand.amber', 'brand.green', 'brand.default', 'brand.blue', 'brand.violet', 'brand.pink', 'brand.gray'];
const DEFAULT_COLOUR = 'brand.default';
const fill = (c: unknown): string => (typeof c === 'string' ? (BRAND[c] ?? (/^#[0-9a-f]{6}$/i.test(c) ? c : DEFAULT_COLOUR)) : DEFAULT_COLOUR);
// design:end palette
export { fill };
