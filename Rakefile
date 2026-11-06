# frozen_string_literal: true

require 'rake/testtask'

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

task default: %i[rubocop check test]
