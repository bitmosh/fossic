# Security

## Reporting a vulnerability

Open a GitHub Issue with `[SECURITY]` in the title. For sensitive disclosures, email the maintainer directly (contact via GitHub profile).

## Security-relevant configuration

`FOSSIC_KEYSTORE_PASSPHRASE` — environment variable used to unlock the fossic keystore (`.age` file). Do not commit this value. Do not log it. Treat it as a secret at the same level as a private key.

If a keystore file is compromised, rotate the passphrase and re-encrypt the keystore. The event log itself does not contain credentials.
