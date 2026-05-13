param(
    [switch] $AllowNetwork,
    [string] $CacheRoot = ".codegraph-bench-cache\real-repos"
)

$ErrorActionPreference = "Stop"

$repos = @(
    @{ id = "typescript-typescript"; url = "https://github.com/microsoft/TypeScript.git"; sha = "68cead182cc24afdc3f1ce7c8ff5853aba14b65a" },
    @{ id = "python-requests"; url = "https://github.com/psf/requests.git"; sha = "0e322af87745eff34caffe4df68456ebc20d9068" },
    @{ id = "go-gin"; url = "https://github.com/gin-gonic/gin.git"; sha = "73726dc606796a025971fe451f0aa6f1b9b847f6" },
    @{ id = "rust-ripgrep"; url = "https://github.com/BurntSushi/ripgrep.git"; sha = "af60c2de9d85e7f3d81c78601669468cf02dabab" },
    @{ id = "java-spring-petclinic"; url = "https://github.com/spring-projects/spring-petclinic.git"; sha = "c7ee170434ec3e369fdc9201290ba2ea4c92b557" }
)

if (-not $AllowNetwork) {
    [pscustomobject]@{
        status = "skipped"
        reason = "network disabled; pass -AllowNetwork to clone real repos"
        cache_root = $CacheRoot
        workflow = "single-agent-only"
    } | ConvertTo-Json -Depth 4
    exit 0
}

New-Item -ItemType Directory -Force -Path $CacheRoot | Out-Null
$results = foreach ($repo in $repos) {
    $target = Join-Path $CacheRoot $repo.id
    if (-not (Test-Path $target)) {
        git clone $repo.url $target
    }
    git -C $target fetch --all --tags
    git -C $target checkout $repo.sha
    $index = codegraph-mcp index $target --profile --json | ConvertFrom-Json
    [pscustomobject]@{
        repo_id = $repo.id
        status = "indexed"
        pinned_commit_sha = $repo.sha
        cache_path = $target
        index = $index
    }
}

[pscustomobject]@{
    status = "completed"
    cache_root = $CacheRoot
    repos = $results
} | ConvertTo-Json -Depth 8

