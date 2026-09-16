import { defineConfig } from "vite";
import { viteSingleFile } from "vite-plugin-singlefile";

// 构建产物是单个自包含 index.html（JS/CSS 内联），由 aludel 二进制 include_str! 嵌入。
// 产物入库管理：CI（纯 Rust）不装 Node。
export default defineConfig({
  plugins: [viteSingleFile()],
  build: {
    outDir: "dist",
    emptyOutDir: true,
    assetsInlineLimit: 100_000_000,
  },
});
