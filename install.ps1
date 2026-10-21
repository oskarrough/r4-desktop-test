# Install or update radio4000 from the latest release.
#   irm https://raw.githubusercontent.com/oskarrough/r4-desktop-test/main/install.ps1 | iex
# The script block keeps its variables and preferences out of the caller's session.
& {
	$ErrorActionPreference = 'Stop'
	# The progress bar makes Invoke-WebRequest crawl on Windows PowerShell 5.1.
	$ProgressPreference = 'SilentlyContinue'

	$repo = 'oskarrough/r4-desktop-test'
	$binDir = if ($env:RADIO4000_BIN_DIR) { $env:RADIO4000_BIN_DIR } else { Join-Path $env:LOCALAPPDATA 'Programs\radio4000' }

	if ($env:PROCESSOR_ARCHITECTURE -ne 'AMD64') {
		throw "radio4000: no build for Windows $env:PROCESSOR_ARCHITECTURE yet. Build from source: https://github.com/$repo#running-from-source"
	}
	$asset = 'radio4000-windows-x86_64.zip'

	$tmp = Join-Path ([IO.Path]::GetTempPath()) ([IO.Path]::GetRandomFileName())
	New-Item -ItemType Directory -Path $tmp | Out-Null
	try {
		Write-Host "Downloading $asset"
		Invoke-WebRequest "https://github.com/$repo/releases/download/latest/$asset" -OutFile "$tmp\$asset" -UseBasicParsing
		Expand-Archive "$tmp\$asset" -DestinationPath "$tmp\new"
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
		Remove-Item -Recurse -Force $tmp
	}
	Write-Host "Installed $binDir\radio4000.exe"

	$userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
	if (($userPath -split ';') -notcontains $binDir) {
		Write-Host "$binDir is not on your PATH. Add it with:"
		Write-Host "  [Environment]::SetEnvironmentVariable('Path', [Environment]::GetEnvironmentVariable('Path', 'User') + ';$binDir', 'User')"
		Write-Host "then open a new terminal."
	}

	# libmpv ships in the zip; yt-dlp is the one thing to check. Install nothing.
	if (-not (Get-Command yt-dlp -ErrorAction SilentlyContinue)) {
		Write-Host "radio4000 needs yt-dlp to play. Run: winget install yt-dlp"
	}
}
