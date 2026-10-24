# CTAO validation package (NSB tree)

Imported from [nsb-validation PR #23](https://github.com/VPRamon/nsb-validation/pull/23)
(`9132acd34b7e2e9359a5664cc91b0943efd48b2a`) and adapted for NSB v0.2.0 where
**Automatic Airglow resolves to PALACE Oxygen**.

```bash
./ctao/reproduce.sh lightweight
./ctao/reproduce.sh full
```

These commands build **this NSB checkout**, verify `Automatic == explicit Oxygen`,
install pinned nsb2 (full mode), and execute `notebooks/NSB_CTAO_Validation.ipynb`.

Scientific admission gates (LVM absolute, Abastumani absolute) are unchanged:
the Oxygen default is a release-policy choice, not a rewritten validation PASS.
