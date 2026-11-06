# frozen_string_literal: true

require 'fileutils'
require 'rake/testtask'

# What install links, to where stow put the Ruby box and its completion before
LINKS = {
  File.join(__dir__, 'target', 'release', 'box') => '~/.local/bin/box',
  File.join(__dir__, 'completions', '_box') => '~/.completions/_box',
}.freeze

# The completion's tests, and the command line's against the debug build with a fake docker
Rake::TestTask.new do |task|
  task.pattern = 'test/**/*_test.rb'
end
task test: :build

desc 'Build box for the tests'
task :build do
  sh 'cargo', 'build'
end

desc 'Check box with rustfmt, clippy and its unit tests'
task :check do
  sh 'cargo', 'fmt', '--check'
  sh 'cargo', 'clippy', '--all-targets', '--', '-D', 'warnings'
  sh 'cargo', 'test'
end

desc 'Run rubocop'
task :rubocop do
  sh 'rubocop'
end

desc 'Build box for release, then link it into ~/.local/bin and its completion into ~/.completions'
task :install do
  sh 'cargo', 'build', '--release'
  LINKS.each do |source, link|
    link = File.expand_path(link)
    FileUtils.mkdir_p(File.dirname(link))
    FileUtils.ln_sf(source, link, verbose: true)
  end
end

task default: %i[rubocop check test]
