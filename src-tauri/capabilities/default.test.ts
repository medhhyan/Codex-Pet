import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { expect, it } from 'vitest';

it('grants the island window permission to start native dragging', () => {
  const capability = JSON.parse(readFileSync(resolve(process.cwd(), 'src-tauri/capabilities/default.json'), 'utf8')) as {
    permissions: string[];
  };

  expect(capability.permissions).toContain('core:window:allow-start-dragging');
});
