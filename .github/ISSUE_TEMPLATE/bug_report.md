---
name: Bug report
about: Something stave does wrong
labels: bug
---

<!--
TENANT DATA CHECK — stave talks to live Wiz tenants. Before
submitting, confirm this report contains NO: tenant ID, region-bearing
api.<region>.app.wiz.io hostname, cloud account/subscription IDs,
resource names/ARNs, user emails, audit-trail lines, or
finding/issue records naming real resources. Sanitization guide:
CONTRIBUTING.md "Sharing logs, payloads, and repros".
-->

**What happened**

**What I expected**

**Repro** (prefer wiremock/fixtures over live-tenant output; run live
repros with `STAVE_AUDIT=off` if sharing a transcript)

```console
$ stave ...
```

**Environment**
- stave version (`stave --version`):
- OS:
- Install path (brew / mise / source):

- [ ] I confirm this report contains no tenant-identifying data
      (tenant ID, region hostname, account/resource IDs, emails,
      payloads, audit lines, secrets).
