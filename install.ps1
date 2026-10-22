# Install or update radio4000 from the latest release.
#   irm https://raw.githubusercontent.com/radio4000/r4-desktop-test/main/install.ps1 | iex
# Adds the install folder to your user PATH; set RADIO4000_NO_MODIFY_PATH=1 to skip that.
# The script block keeps its variables and preferences out of the caller's session.
& {
	$ErrorActionPreference = 'Stop'
	# The progress bar makes Invoke-WebRequest crawl on Windows PowerShell 5.1.
	$ProgressPreference = 'SilentlyContinue'

	$repo = 'radio4000/r4-desktop-test'
	$binDir = if ($env:RADIO4000_BIN_DIR) { $env:RADIO4000_BIN_DIR } else { Join-Path $env:LOCALAPPDATA 'Programs\radio4000' }

	# A 32-bit PowerShell on 64-bit Windows reports x86 here and the real machine in PROCESSOR_ARCHITEW6432.
	$arch = if ($env:PROCESSOR_ARCHITEW6432) { $env:PROCESSOR_ARCHITEW6432 } else { $env:PROCESSOR_ARCHITECTURE }
	if ($arch -ne 'AMD64') {
		throw "radio4000: no build for Windows $arch yet. Build from source: https://github.com/$repo#running-from-source"
	}
	$asset = 'radio4000-windows-x86_64.zip'

	# radio4000.exe is a windowed app: Windows PowerShell neither waits for it nor reads what it
	# prints, so it runs through Start-Process into a file. Nothing comes back from a build that won't run.
	function Get-Version($exe) {
		$out = Join-Path $tmp 'version.txt'
		try {
			$process = Start-Process $exe -ArgumentList '--version' -NoNewWindow -Wait -PassThru -RedirectStandardOutput $out
		} catch { return }
		if ($process.ExitCode -eq 0 -and "$(Get-Content $out -Raw)" -match '^radio4000 (\S+)') { $Matches[1] }
	}

	$tmp = Join-Path ([IO.Path]::GetTempPath()) ([IO.Path]::GetRandomFileName())
	New-Item -ItemType Directory -Path $tmp | Out-Null
	try {
		Write-Host "Downloading $asset"
		$url = "https://github.com/$repo/releases/download/latest"
		Invoke-WebRequest "$url/SHA256SUMS" -OutFile "$tmp\SHA256SUMS" -UseBasicParsing
		Invoke-WebRequest "$url/$asset" -OutFile "$tmp\$asset" -UseBasicParsing
		$line = Get-Content "$tmp\SHA256SUMS" | Where-Object { $_ -match "\s\*?$([regex]::Escape($asset))$" } | Select-Object -First 1
		if (-not $line) { throw "radio4000: $asset is not listed in SHA256SUMS" }
		$expected = ($line -split '\s+')[0].ToLowerInvariant()
		$actual = (Get-FileHash -Algorithm SHA256 "$tmp\$asset").Hash.ToLowerInvariant()
		if ($actual -ne $expected) { throw "radio4000: checksum mismatch for $asset; nothing installed" }
		Expand-Archive "$tmp\$asset" -DestinationPath "$tmp\new"

		# Make sure the new build runs (libmpv ships beside it) before it replaces the old one.
		$newVersion = Get-Version "$tmp\new\radio4000.exe"
		if (-not $newVersion) { throw "radio4000: the downloaded radio4000 doesn't run; kept the old install" }
		# Builds from before --version existed print something else; treat them as unknown.
		$oldVersion = $null
		if (Test-Path "$binDir\radio4000.exe") {
			$oldVersion = Get-Version "$binDir\radio4000.exe"
			if (-not $oldVersion) { $oldVersion = '(older build)' }
		}

		# Install everything the zip holds (the exe and the mpv DLL it loads), replacing the old copy.
		# Windows won't overwrite a running exe or loaded DLL but will rename it, so the old file
		# moves aside to .old first. If an older .old is still running, the current file isn't, and
		# is overwritten directly.
		New-Item -ItemType Directory -Force -Path $binDir | Out-Null
		foreach ($file in Get-ChildItem "$tmp\new" -Recurse -File) {
			$dest = Join-Path $binDir $file.FullName.Substring("$tmp\new".Length + 1)
			New-Item -ItemType Directory -Force -Path (Split-Path $dest) | Out-Null
			if (Test-Path $dest) {
				Remove-Item "$dest.old" -Force -ErrorAction SilentlyContinue
				if (-not (Test-Path "$dest.old")) { Move-Item $dest "$dest.old" }
			}
			Move-Item $file.FullName $dest -Force
		}
	} finally {
		# Quietly, so a failure above isn't replaced by a file Windows still holds.
		Remove-Item -Recurse -Force $tmp -ErrorAction SilentlyContinue
	}
	# A Start Menu entry, so the app opens from Start and search like any other. The exe holds the icon.
	$shortcut = (New-Object -ComObject WScript.Shell).CreateShortcut((Join-Path ([Environment]::GetFolderPath('Programs')) 'Radio4000.lnk'))
	$shortcut.TargetPath = "$binDir\radio4000.exe"
	$shortcut.WorkingDirectory = $binDir
	$shortcut.Save()

	if ($oldVersion -eq $newVersion) {
		Write-Host "Reinstalled radio4000 $newVersion in $binDir"
	} elseif ($oldVersion) {
		Write-Host "Updated radio4000 $oldVersion -> $newVersion in $binDir"
	} else {
		Write-Host "Installed radio4000 $newVersion in $binDir"
	}

	# Read and write the raw registry value: going through [Environment] would expand entries
	# like %USERPROFILE%\... in the rest of the user's PATH and save them hard-coded.
	$envKey = 'HKCU:\Environment'
	$userPath = (Get-Item $envKey).GetValue('Path', '', 'DoNotExpandEnvironmentNames')
	if (($userPath -split ';') -notcontains $binDir) {
		if ($env:RADIO4000_NO_MODIFY_PATH -eq '1') {
			Write-Host "$binDir is not on your PATH. Add it under Start > Edit environment variables for your account."
		} else {
			$newPath = if ($userPath) { "$($userPath.TrimEnd(';'));$binDir" } else { $binDir }
			Set-ItemProperty $envKey -Name Path -Value $newPath -Type ExpandString
			# Setting and clearing a dummy variable broadcasts the change, so new terminals see it.
			$dummy = "radio4000-$([guid]::NewGuid())"
			[Environment]::SetEnvironmentVariable($dummy, '1', 'User')
			[Environment]::SetEnvironmentVariable($dummy, [NullString]::Value, 'User')
			Write-Host "Added $binDir to your user PATH."
		}
	}
	# This terminal reads PATH once at start, so add it here too unless opted out.
	if ($env:RADIO4000_NO_MODIFY_PATH -ne '1' -and ($env:Path -split ';') -notcontains $binDir) {
		$env:Path = "$env:Path".TrimEnd(';') + ";$binDir"
	}
}
