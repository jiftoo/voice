// vite.config.ts
import { defineConfig } from "file:///D:/Coding/rust/voice/web2.0/node_modules/.pnpm/vite@5.0.11/node_modules/vite/dist/node/index.js";
import solid from "file:///D:/Coding/rust/voice/web2.0/node_modules/.pnpm/vite-plugin-solid@2.8.0_solid-js@1.8.11_vite@5.0.11/node_modules/vite-plugin-solid/dist/esm/index.mjs";
import mkcert from "file:///D:/Coding/rust/voice/web2.0/node_modules/.pnpm/vite-plugin-mkcert@1.17.3_vite@5.0.11/node_modules/vite-plugin-mkcert/dist/mkcert.mjs";
var vite_config_default = defineConfig({
  plugins: [solid(), mkcert()],
  server: {
    https: true,
    open: true,
    port: 3e3
  }
});
export {
  vite_config_default as default
};
//# sourceMappingURL=data:application/json;base64,ewogICJ2ZXJzaW9uIjogMywKICAic291cmNlcyI6IFsidml0ZS5jb25maWcudHMiXSwKICAic291cmNlc0NvbnRlbnQiOiBbImNvbnN0IF9fdml0ZV9pbmplY3RlZF9vcmlnaW5hbF9kaXJuYW1lID0gXCJEOlxcXFxDb2RpbmdcXFxccnVzdFxcXFx2b2ljZVxcXFx3ZWIyLjBcIjtjb25zdCBfX3ZpdGVfaW5qZWN0ZWRfb3JpZ2luYWxfZmlsZW5hbWUgPSBcIkQ6XFxcXENvZGluZ1xcXFxydXN0XFxcXHZvaWNlXFxcXHdlYjIuMFxcXFx2aXRlLmNvbmZpZy50c1wiO2NvbnN0IF9fdml0ZV9pbmplY3RlZF9vcmlnaW5hbF9pbXBvcnRfbWV0YV91cmwgPSBcImZpbGU6Ly8vRDovQ29kaW5nL3J1c3Qvdm9pY2Uvd2ViMi4wL3ZpdGUuY29uZmlnLnRzXCI7aW1wb3J0IHtkZWZpbmVDb25maWd9IGZyb20gXCJ2aXRlXCI7XG5pbXBvcnQgc29saWQgZnJvbSBcInZpdGUtcGx1Z2luLXNvbGlkXCI7XG5pbXBvcnQgbWtjZXJ0IGZyb20gXCJ2aXRlLXBsdWdpbi1ta2NlcnRcIjtcblxuZXhwb3J0IGRlZmF1bHQgZGVmaW5lQ29uZmlnKHtcblx0cGx1Z2luczogW3NvbGlkKCksIG1rY2VydCgpXSxcblx0c2VydmVyOiB7XG5cdFx0aHR0cHM6IHRydWUsXG5cdFx0b3BlbjogdHJ1ZSxcblx0XHRwb3J0OiAzMDAwXG5cdH1cbn0pO1xuIl0sCiAgIm1hcHBpbmdzIjogIjtBQUE2USxTQUFRLG9CQUFtQjtBQUN4UyxPQUFPLFdBQVc7QUFDbEIsT0FBTyxZQUFZO0FBRW5CLElBQU8sc0JBQVEsYUFBYTtBQUFBLEVBQzNCLFNBQVMsQ0FBQyxNQUFNLEdBQUcsT0FBTyxDQUFDO0FBQUEsRUFDM0IsUUFBUTtBQUFBLElBQ1AsT0FBTztBQUFBLElBQ1AsTUFBTTtBQUFBLElBQ04sTUFBTTtBQUFBLEVBQ1A7QUFDRCxDQUFDOyIsCiAgIm5hbWVzIjogW10KfQo=
