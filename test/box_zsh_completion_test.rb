# frozen_string_literal: true

require_relative 'test_helper'

class BoxZshCompletionTest < Minitest::Test
  COMPLETIONS = File.join(ROOT, 'completions', 'zsh')

  # Loads the completion, with a _describe that prints the candidates, and completes the last word
  SCRIPT = <<~'ZSH'
    fpath=("$1" $fpath)
    shift
    autoload -Uz _box
    _describe() {
      while [[ $1 == -? ]]; do shift 2; done
      shift
      local name
      for name; do
        (( ${#${(P)name}} )) && print -rl -- "${(@)${(@P)name}%%:*}"
      done
      return 0
    }
    words=("$@")
    CURRENT=$#
    _box
    exit 0
  ZSH

  def setup
    skip 'zsh is not installed' unless zsh?
  end

  def test_offers_commands_then_targets
    assert_equal %w[list prune stop], complete('')
    assert_equal %w[containers images volumes], complete('list', '')
    assert_equal %w[containers images volumes all], complete('prune', '')
    assert_empty complete('list', 'images', '')
    assert_empty complete('stop', '')
  end

  def test_offers_options_for_a_dash
    assert_equal %w[-h --help -V --version], complete('-')
    assert_empty complete('--version', '')
    assert_equal %w[-h --help -x --extended], complete('list', 'containers', '-')
    assert_equal %w[-h --help], complete('list', 'images', '-')
    assert_equal %w[containers], complete('list', '-x', '')
    assert_empty complete('list', '--help', '')
  end

  def test_prune_offers_the_targets_not_given_yet
    assert_equal %w[containers volumes], complete('prune', 'images', '')
    assert_empty complete('prune', 'all', '')
  end

  private

  def zsh?
    ENV.fetch('PATH', '').split(File::PATH_SEPARATOR).any? { |dir| File.executable?(File.join(dir, 'zsh')) }
  end

  def complete(*words)
    stdout, stderr, status = Open3.capture3('zsh', '-f', '-c', SCRIPT, 'zsh', COMPLETIONS, 'box', *words)
    assert status.success?, stderr
    assert_empty stderr

    stdout.lines(chomp: true)
  end
end
