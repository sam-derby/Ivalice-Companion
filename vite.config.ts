import react from '@vitejs/plugin-react';
import { configDefaults, defineConfig } from 'vitest/config';

export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: { host: 'localhost', port: 1420, strictPort: true },
  test: {
    environment: 'jsdom',
    setupFiles: ['./src/test-setup.ts'],
    // Ignored private checkouts under .local are not workspace tests.
    exclude: [...configDefaults.exclude, '.local/**'],
  },
});
