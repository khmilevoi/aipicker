$ErrorActionPreference = 'Stop'
# Verify the resolved native integration, not merely text in Cargo.toml.
$features = & cargo tree --locked --offline -e features -i egui-winit 2>&1
if ($LASTEXITCODE -ne 0) { throw 'Could not inspect resolved egui-winit features' }
if (!(($features -join "`n") -match 'egui-winit feature "links"')) {
    throw 'Native URL opening is disabled: egui-winit links feature is missing'
}
'PASS: native URL opening is enabled in the resolved dependency graph'
