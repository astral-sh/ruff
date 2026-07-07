# Typeshed patches

These patches are applied by the automated sync workflow after docstrings are added and the stubs
are formatted. Each patch is rooted at the typeshed repository and must apply cleanly. The workflow
pushes the unpatched sync branch before applying them so that a failed patch can be fixed manually
without rerunning the full sync.
