param(
  [Parameter(Mandatory = $true, Position = 0)]
  [string]$FilePath
)

$ErrorActionPreference = 'Stop'

# Unsigned builds are supported: do not fail when no Windows signing option is configured.
if ($env:WINDOWS_CERTIFICATE) {
  if (-not $env:WINDOWS_CERTIFICATE_PASSWORD) {
    throw 'WINDOWS_CERTIFICATE_PASSWORD is required when WINDOWS_CERTIFICATE is set.'
  }

  $certificatePath = Join-Path $env:RUNNER_TEMP 'pdm-code-signing.pfx'
  try {
    [IO.File]::WriteAllBytes($certificatePath, [Convert]::FromBase64String($env:WINDOWS_CERTIFICATE))
    $signtool = Get-ChildItem "${env:ProgramFiles(x86)}\Windows Kits\10\bin\*\x64\signtool.exe" |
      Sort-Object FullName -Descending |
      Select-Object -First 1 -ExpandProperty FullName
    if (-not $signtool) { throw 'signtool.exe was not found on this runner.' }
    & $signtool sign /fd SHA256 /tr 'http://timestamp.digicert.com' /td SHA256 /f $certificatePath /p $env:WINDOWS_CERTIFICATE_PASSWORD $FilePath
    if ($LASTEXITCODE -ne 0) { throw "signtool failed with exit code $LASTEXITCODE." }
  }
  finally {
    Remove-Item $certificatePath -Force -ErrorAction SilentlyContinue
  }
  exit 0
}

if ($env:AZURE_CLIENT_ID -and $env:AZURE_CLIENT_SECRET -and $env:AZURE_TENANT_ID -and
    $env:AZURE_TRUSTED_SIGNING_ENDPOINT -and $env:AZURE_TRUSTED_SIGNING_ACCOUNT -and $env:AZURE_TRUSTED_SIGNING_CERTIFICATE_PROFILE) {
  & artifact-signing-cli -e $env:AZURE_TRUSTED_SIGNING_ENDPOINT -a $env:AZURE_TRUSTED_SIGNING_ACCOUNT -c $env:AZURE_TRUSTED_SIGNING_CERTIFICATE_PROFILE -d 'Pencil Download Manager' $FilePath
  if ($LASTEXITCODE -ne 0) { throw "Azure Trusted Signing failed with exit code $LASTEXITCODE." }
}
