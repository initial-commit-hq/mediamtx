# Camera Grid Viewer

MediaMTX includes a built-in web-based camera grid viewer that automatically discovers all active streams and displays them in a configurable grid layout.

## Enabling the viewer

Enable the viewer in the configuration file:

```yml
viewer: yes
viewerAddress: :9999
```

Then open `http://<server-ip>:9999/` in a browser. The viewer automatically detects the server's IP from the URL and connects to the API on port 9997.

## Features

- **Auto-connect** — connects to the MediaMTX API on page load using the host it was served from
- **WebRTC first** — attempts WebRTC (WHEP) for each stream, with a 6-second timeout before falling back to HLS
- **Grid layouts** — switch between 1×1, 2×2, and 3×3 grids
- **Stream selector** — when more than 9 streams are active, a selector bar lets you choose which ones to display
- **Pagination** — navigate through streams in pages matching the selected grid size

## Configuration

| Parameter | Default | Description |
|-----------|---------|-------------|
| `viewer` | `no` | Enable the viewer server |
| `viewerAddress` | `:9999` | Address the viewer listens on |

The viewer uses the browser's `window.location.hostname` to determine the MediaMTX host, so it works correctly when accessed from any network interface. The API port (default `9997`), WebRTC port (default `8889`), and HLS port (default `8888`) can be adjusted in the viewer's UI if they differ from the defaults.

## CORS

The viewer connects directly to the MediaMTX API and media endpoints from the browser. Ensure the API and WebRTC/HLS servers have appropriate `allowOrigins` configured if the viewer is served from a different port:

```yml
apiAllowOrigins:
  - '*'
```