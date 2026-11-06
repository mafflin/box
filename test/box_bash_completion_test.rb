# frozen_string_literal: true

require_relative 'test_helper'

class BoxBashCompletionTest < Minitest::Test
  COMPLETION = File.join(ROOT, 'completions', 'bash', '_box')

  # Sources the completion, completes the last word and prints the candidates
  SCRIPT = <<~'BASH'
    source "$1"
    shift
    COMP_WORDS=("$@")
    COMP_CWORD=$(( $# - 1 ))
    _box
    (( ${#COMPREPLY[@]} == 0 )) || printf '%s\n' "${COMPREPLY[@]}"
  BASH

  def test_offers_commands_then_targets
    assert_equal %w[list prune stop], complete('')
    assert_equal %w[containers images volumes], complete('list', '')
    assert_equal %w[containers images volumes all], complete('prune', '')
    assert_empty complete('list', 'images', '')
    assert_empty complete('stop', '')
  end

  def test_offers_options_for_a_dash
    assert_equal %w[-h --help], complete('-')
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

  def complete(*words)
    stdout, stderr, status = Open3.capture3('bash', '-c', SCRIPT, 'bash', COMPLETION, 'box', *words)
    assert status.success?, stderr

    stdout.lines(chomp: true)
  end
end
