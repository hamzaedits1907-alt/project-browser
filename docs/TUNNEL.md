# Project Tunnel

**Status in 0.4.0: a browser proxy you supply. There is no VPN service, no system-wide tunnel and no provider.**

## What is real

* You enter an HTTP/HTTPS or SOCKS5 proxy (host and port) in Settings → Project Tunnel. After a restart, this browser's
  traffic goes to that proxy. The proxy is passed to WebView2 as start-up arguments (`--proxy-server`), so it covers every
  window, tab and popup, and only them.
* Pages on this computer never use the proxy (`--proxy-bypass-list`). WebRTC is limited to routes through the proxy
  (`--force-webrtc-ip-handling-policy=disable_non_proxied_udp`) so a page cannot find the real address through a direct UDP
  connection. Web addresses are resolved by the proxy, not locally (Chromium sends host names to HTTP and SOCKS5 proxies).
* **The status is honest.** `Active` means only: the running browser was started with exactly this proxy
  (`tunnel::state`, unit-tested). Saving settings shows "Restart needed" until the browser has actually restarted. "Test the
  proxy" speaks the proxy's protocol in the open (an HTTP `CONNECT` to example.com:443, or a SOCKS5 greeting) and reports
  what came back: works, refused with the reason, or unreachable. It does not say where the traffic comes out.

## What is not there

* **No system-wide VPN.** That needs a tunnel driver (WireGuard/Wintun or OpenVPN), a signed helper running with
  administrator rights, route and DNS changes, a firewall kill switch and a server. None of it exists here. The settings page
  lists it as "Not available" with this reason, so nothing implies it works.
* **No provider.** No servers, locations, accounts or credentials are built in or invented. A VPN company's service would need
  their API and servers.
* **No proxy passwords.** WebView2 reports a proxy's password prompt through the same event as a web site's, so answering
  it could hand the proxy password to a web site. Unauthenticated proxies (for example address-allowlisted ones) only.
* **No kill switch claim.** Chromium does not fall back to a direct connection when a fixed proxy fails, so pages fail
  instead of leaking; but that has not been observed in this browser, and nothing else (other programs, Windows, the
  updater) is covered.
* **Leaks not tested:** DNS and IP leak checks need a real proxy and a leak-test page; the steps are in
  `docs/VERIFY-WINDOWS.md` (T1–T4). Until someone runs them, treat the leak protections above as intended, not proven.

## To make a real VPN later

1. Pick a provider or protocol (for WireGuard: a provider's configs, or the user's own server).
2. A Windows service (signed, installed with consent) that owns the Wintun adapter, routes and firewall rules, with the
   browser talking to it over a local authenticated pipe.
3. A state machine driven by the service's reports (connecting, connected, kill switch armed, failed), crash cleanup that
   removes routes and firewall rules, and leak tests in CI-independent manual checks.
