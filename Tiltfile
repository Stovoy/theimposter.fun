def nvm_cmd(inner):
    return (
        "bash -lc '"
        "export NVM_DIR=\"${NVM_DIR:-$HOME/.nvm}\"; "
        "[ -s \"$NVM_DIR/nvm.sh\" ] && source \"$NVM_DIR/nvm.sh\"; "
        + inner
        + "'"
    )


local_resource(
    name='backend-deps',
    cmd='cd backend && cargo fetch',
    deps=['backend/Cargo.toml', 'backend/Cargo.lock'],
)

local_resource(
    name='backend',
    serve_cmd='cd backend && cargo run',
    deps=[
        'backend/Cargo.toml',
        'backend/Cargo.lock',
        'backend/src',
        'backend/data',
    ],
    resource_deps=['backend-deps'],
)

local_resource(
    name='frontend-deps',
    cmd=nvm_cmd('cd frontend && nvm use >/dev/null && npm install'),
    deps=['frontend/package.json', 'frontend/package-lock.json'],
)

local_resource(
    name='frontend',
    serve_cmd=nvm_cmd(
        'cd frontend && nvm use >/dev/null && npm run dev -- --host 0.0.0.0 --port 5173'
    ),
    deps=[
        'frontend/package.json',
        'frontend/package-lock.json',
        'frontend/src',
        'frontend/public',
        'frontend/vite.config.ts',
        'frontend/svelte.config.js',
        'frontend/tsconfig.json',
        'frontend/tsconfig.app.json',
        'frontend/tsconfig.node.json',
    ],
    resource_deps=['frontend-deps'],
)
