<#
.SYNOPSIS
    Exports all of your Microsoft To Do lists and tasks (including steps) to a JSON
    file that RustDo can import.

.DESCRIPTION
    Uses Microsoft's official Microsoft Graph PowerShell module to sign in and read
    your tasks. It only asks for the read-only "Tasks.Read" permission and doesn't
    change anything in your Microsoft account.

.PARAMETER OutFile
    Where to write the export. Defaults to mstodo-export.json on your Desktop.

.PARAMETER TenantId
    "common" works for most accounts. If sign-in with a personal Microsoft account
    (outlook.com / hotmail.com / live.com) fails, try -TenantId consumers.

.PARAMETER UseDeviceCode
    Sign in with a code at https://microsoft.com/devicelogin instead of a pop-up window.

.EXAMPLE
    powershell -ExecutionPolicy Bypass -File .\tools\Export-MicrosoftToDo.ps1
#>
param(
    [string]$OutFile = (Join-Path ([Environment]::GetFolderPath('Desktop')) 'mstodo-export.json'),
    [string]$TenantId = 'common',
    [switch]$UseDeviceCode
)

$ErrorActionPreference = 'Stop'

if (-not (Get-Module -ListAvailable -Name Microsoft.Graph.Authentication)) {
    Write-Host 'Installing the Microsoft.Graph.Authentication module (current user only)...'
    Install-Module Microsoft.Graph.Authentication -Scope CurrentUser -Force -AllowClobber
}
Import-Module Microsoft.Graph.Authentication

$connect = @{ Scopes = 'Tasks.Read'; TenantId = $TenantId; NoWelcome = $true }
if ($UseDeviceCode) { $connect.UseDeviceCode = $true }
Connect-MgGraph @connect

# Follows @odata.nextLink so long lists are fetched completely.
function Get-AllPages([string]$Uri) {
    $items = New-Object System.Collections.ArrayList
    while ($Uri) {
        $page = Invoke-MgGraphRequest -Method GET -Uri $Uri -OutputType PSObject
        foreach ($item in @($page.value)) { if ($null -ne $item) { [void]$items.Add($item) } }
        $Uri = $page.'@odata.nextLink'
    }
    return ,$items
}

try {
    $lists = Get-AllPages 'https://graph.microsoft.com/v1.0/me/todo/lists'
    $out = New-Object System.Collections.ArrayList
    $total = 0
    foreach ($list in $lists) {
        $tasks = Get-AllPages "https://graph.microsoft.com/v1.0/me/todo/lists/$($list.id)/tasks?`$expand=checklistItems"
        $total += $tasks.Count
        Write-Host ("  {0,-40} {1,5} tasks" -f $list.displayName, $tasks.Count)
        [void]$out.Add([ordered]@{
            id                = $list.id
            displayName       = $list.displayName
            wellknownListName = $list.wellknownListName
            tasks             = $tasks
        })
    }

    $doc = [ordered]@{
        source     = 'microsoft-graph'
        exportedAt = (Get-Date).ToString('o')
        lists      = $out
    }
    $doc | ConvertTo-Json -Depth 20 | Out-File -FilePath $OutFile -Encoding utf8
    Write-Host ''
    Write-Host "Exported $($lists.Count) lists and $total tasks to $OutFile"
    Write-Host 'In RustDo, click "Import from Microsoft To Do..." (or drop the file onto the window).'
}
finally {
    Disconnect-MgGraph | Out-Null
}
