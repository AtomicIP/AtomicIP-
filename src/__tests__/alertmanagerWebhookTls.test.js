/**
 * Issue #1260: Alertmanager incident webhooks must use TLS.
 * Plaintext http:// webhook URLs expose alert payloads and bearer tokens
 * to anything on the pod network.
 */
const fs = require('fs');
const path = require('path');

const CONFIG_PATH = path.join(__dirname, '../../monitoring/alertmanager/alertmanager.yml');

function readWebhookUrls(text) {
  const urls = [];
  const lines = text.split('\n');
  for (const line of lines) {
    const match = line.match(/^\s*-?\s*url:\s*(\S+)\s*$/);
    if (match) urls.push(match[1].replace(/^['"]|['"]$/g, ''));
  }
  return urls;
}

describe('Alertmanager webhook transport (#1260)', () => {
  const text = fs.readFileSync(CONFIG_PATH, 'utf8');
  const urls = readWebhookUrls(text);

  test('config declares at least one webhook URL', () => {
    expect(urls.length).toBeGreaterThan(0);
  });

  test('no webhook URL uses plaintext http://', () => {
    const plaintext = urls.filter((u) => /^http:\/\//i.test(u));
    expect(plaintext).toEqual([]);
  });

  test('every webhook URL uses https://', () => {
    for (const url of urls) {
      expect(url).toMatch(/^https:\/\//);
    }
  });

  test('incident-tracker receiver targets the TLS endpoint', () => {
    const block = text.split(/\n\s*- name: /).find((b) => b.startsWith('incident-tracker'));
    expect(block).toBeDefined();
    expect(block).toMatch(/url:\s*https:\/\/atomicip-api\.atomicip-production\.svc\//);
  });

  test('the helper rejects http:// URLs when they appear in config', () => {
    const sample = '  webhook_configs:\n      - url: http://example.svc/hook\n';
    expect(readWebhookUrls(sample)).toEqual(['http://example.svc/hook']);
  });
});
