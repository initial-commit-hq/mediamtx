// https://nuxt.com/docs/api/configuration/nuxt-config
export default defineNuxtConfig({
  ssr: false,

  // Output static files to internal/viewerserver/dist/ so Go can embed them
  // (go:embed paths are relative to the package directory, not the repo root)
  generate: {
    dir: '../internal/viewerserver/dist',
  },

  // Belt-and-suspenders: also tell Nitro where the public dir lives
  nitro: {
    output: {
      publicDir: '../../internal/viewerserver/dist',
    },
  },

  app: {
    baseURL: '/',
    head: {
      title: 'Camera Grid — MediaMTX',
      meta: [
        { charset: 'utf-8' },
        { name: 'viewport', content: 'width=device-width, initial-scale=1.0' },
      ],
    },
  },

  css: ['~/assets/css/main.css'],

  compatibilityDate: '2024-11-01',
})
