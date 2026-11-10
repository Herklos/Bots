// Renders the brand's SVG files to PNG with Chromium: the app icon the app.json names, and
// previews for looking at. Run from the repository root after `hbots-design sync`:
//   node design/scripts/render.cjs [preview-dir]
const { chromium } = require('playwright');
const fs = require('fs');
const path = require('path');

(async () => {
  const root = path.resolve(__dirname, '../..');
  const browser = await chromium.launch();
  const shot = async (svg, out, width, height, background, scheme) => {
    const page = await browser.newPage({ viewport: { width, height }, colorScheme: scheme ?? 'light' });
    const data = fs.readFileSync(path.join(root, svg)).toString('base64');
    await page.setContent(`<body style="margin:0;background:${background}"><img style="display:block;width:${width}px;height:${height}px" src="data:image/svg+xml;base64,${data}">`);
    await page.screenshot({ path: out, omitBackground: background === 'transparent' });
    await page.close();
  };
  await shot('design/assets/app-icon.svg', path.join(root, 'app/assets/icon-1024.png'), 1024, 1024, 'transparent');
  const preview = process.argv[2];
  if (preview) {
    fs.mkdirSync(preview, { recursive: true });
    await shot('design/assets/lockup.svg', path.join(preview, 'lockup-light.png'), 660, 200, '#ffffff', 'light');
    await shot('design/assets/lockup.svg', path.join(preview, 'lockup-dark.png'), 660, 200, '#0d1117', 'dark');
    await shot('design/assets/mark.svg', path.join(preview, 'mark-dark.png'), 200, 200, '#0d1117', 'dark');
    await shot('design/assets/favicon.svg', path.join(preview, 'favicon.png'), 64, 64, '#ffffff', 'light');
  }
  await browser.close();
})();
