import type { ElectrobunConfig } from "electrobun";

export default {
	app: {
		name: "mediamtx",
		identifier: "mediamtx.electrobun.dev",
		version: "0.0.1",
	},
	build: {
		// Vite builds to dist/, we copy from there
		copy: {
			"dist/index.html": "views/mainview/index.html",
			"dist/assets": "views/mainview/assets",
		},
		// Ignore Vite output in watch mode — HMR handles view rebuilds separately
		watchIgnore: ["dist/**"],
		mac: {
			bundleCEF: true,
            icon: "assets/icon.png",
		},
		linux: {
			bundleCEF: false,
		},
		win: {
			bundleCEF: false,
		},
	},
} satisfies ElectrobunConfig;
