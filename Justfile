
test:
  cargo t

publish:
  cargo +nightly -Z package-workspace publish --workspace
