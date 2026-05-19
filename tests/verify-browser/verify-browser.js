const { chromium } = require('playwright');

(async () => {
  const sessionId = 'bc76dd47-a83a-480a-83d9-42a213e0cf35';
  const wsUrl = `ws://172.16.0.2:3000/connect?sessionId=${sessionId}`;
  
  console.log(`Connecting to Steel WebSocket: ${wsUrl}...`);
  
  try {
    const browser = await chromium.connect(wsUrl);
    
    console.log('Connected! Creating new page...');
    const context = await browser.newContext();
    const page = await context.newPage();
    
    console.log('Navigating to https://google.com...');
    await page.goto('https://google.com', { waitUntil: 'networkidle' });
    
    console.log('Page title:', await page.title());
    
    console.log('Taking screenshot...');
    await page.screenshot({ path: 'resources/browser-screenshot.png' });
    
    console.log('Success! Screenshot saved to resources/browser-screenshot.png');
    
    await browser.close();
  } catch (err) {
    console.error('Connection failed:', err.message);
    process.exit(1);
  }
})();
