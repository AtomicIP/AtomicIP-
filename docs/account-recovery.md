# Account recovery email integration

The API sends recovery messages through a configured HTTPS JSON mail endpoint.
Set these environment variables on the API server:

- `RECOVERY_EMAIL_API_URL`: HTTPS endpoint accepting the request below.
- `RECOVERY_EMAIL_API_KEY`: bearer token sent in the `Authorization` header.
- `RECOVERY_FROM_EMAIL`: sender address included in the request body.

The endpoint receives a `POST` with `Content-Type: application/json` and a
JSON body containing `from`, `to`, `subject`, and `text`. Any 2xx response
indicates that the provider accepted the message. Missing configuration,
transport errors, and non-2xx responses return `503`; the API does not claim
that an email was sent in those cases.

Recovery tokens are random, stored as SHA-256 hashes, expire after 15 minutes,
and can be used only once. Token and lockout state currently live in process
memory and are not shared across API instances. Recovery attempts are limited
to five per normalized email address before a 30-minute lockout.

The repository does not yet contain an authoritative account/password store.
Consequently, token verification consumes a valid token but returns `501`
instead of falsely reporting a successful password reset. Password changes
remain unavailable until that credential-store integration is implemented.
