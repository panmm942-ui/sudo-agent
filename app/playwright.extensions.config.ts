import {defineConfig} from '@playwright/test';
export default defineConfig({testDir:'tests/extensions',workers:1,reporter:'list',timeout:30000,use:{baseURL:'http://127.0.0.1:4395',browserName:'chromium',headless:true},webServer:{command:'npx vite dev --host 127.0.0.1 --port 4395 --strictPort',url:'http://127.0.0.1:4395',reuseExistingServer:true,timeout:120000}});
