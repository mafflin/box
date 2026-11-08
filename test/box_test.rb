# frozen_string_literal: true

require_relative 'test_helper'

class BoxTest < Minitest::Test
  # The debug build unless BOX_BIN names another
  BOX = ENV.fetch('BOX_BIN', File.join(ROOT, 'target', 'debug', 'box'))
  FAKE_BIN = File.join(ROOT, 'test', 'fake_bin')

  def setup
    @tmp_dir = Dir.mktmpdir
    @home = File.join(@tmp_dir, 'home')
    @responses = []
  end

  def teardown
    FileUtils.remove_entry(@tmp_dir)
  end

  def test_prints_help_without_arguments
    stdout, _, status = box

    assert status.success?
    assert_match(/\AUsage: box \[command\] \[options\]$/, stdout)
    assert_match(/\AUsage: box list WHAT/, box('list').first)
    assert_empty docker_calls
  end

  def test_rejects_invalid_arguments
    {
      %w[foo] => "unknown command: foo\nTry 'box --help'",
      %w[list foo] => "unknown target: foo\nTry 'box list --help'",
      %w[list images -x] => '--extended only works with containers',
      %w[prune all -z] => 'invalid option: -z',
      %w[stop now] => 'unexpected argument: now',
    }.each do |args, message|
      _, stderr, status = box(*args)

      refute status.success?, args
      assert_includes stderr, "box: #{message}", args
    end
    assert_empty docker_calls
  end

  def test_fails_when_docker_is_not_installed
    _, stderr, status = box('stop', path: File.dirname(RbConfig.ruby))

    refute status.success?
    assert_equal "box: docker is not installed\n", stderr
  end

  def test_fails_when_docker_is_not_running
    respond('info', status: 1)

    _, stderr, status = box('stop')

    refute status.success?
    assert_equal "box: docker is not running\n", stderr
  end

  def test_prunes_in_a_fixed_order_once
    respond('ps', '--filter', 'status=exited', '--filter', 'status=created', '-q', stdout: "c1\n")
    respond('volume', 'ls', '--filter', 'dangling=true', '-q', stdout: "v1\n")
    respond('images', '--filter', 'dangling=true', '-q', '--no-trunc', stdout: "i1\n")

    _, _, status = box('prune', 'images', 'containers', 'all')

    assert status.success?
    assert_equal [%w[rm -v c1], %w[volume rm v1], %w[rmi i1]], changes
  end

  def test_prune_leaves_dangling_images_that_containers_use
    respond('images', '--filter', 'dangling=true', '-q', '--no-trunc', stdout: "#{image_id('a')}\n#{image_id('b')}\n")
    containers('web' => [image_id('a')])

    _, _, status = box('prune', 'images')

    assert status.success?
    assert_equal [['rmi', image_id('b')]], changes
  end

  def test_aborts_on_first_failed_docker_command
    running('web', 'db')
    respond('stop', 'web-id', status: 1)

    _, stderr, status = box('stop')

    refute status.success?
    assert_equal "box: docker stop web-id failed\n", stderr
    assert_equal [%w[stop web-id]], changes
  end

  def test_list_containers_prints_a_table
    running(
      'web' => details(ports: { '80/tcp' => [{ 'HostPort' => '8080' }, { 'HostPort' => '8080' }], '443/tcp' => nil },
                       status: 'Up 2 days', mounts: [volume('html', '/srv/html')]),
      'app' => details(ips: %w[172.18.0.3 172.19.0.4], image: 'app:dev', user: 'ubuntu', workdir: '/app',
                       status: 'Up 45 minutes (healthy)',
                       mounts: [bind("#{@home}/src/app", '/app'), bind('/etc/hosts', '/etc/hosts', writable: false)]),
    )

    assert_equal <<~TABLE, plain(box('list', 'containers').first)
      NAME  IMAGE       USER    STATUS                   PORTS         WORKDIR
      web   nginx:1.27  root    Up 2 days                443/tcp       /
                                                         8080->80/tcp
      app   app:dev     ubuntu  Up 45 minutes (healthy)                /app
    TABLE
    assert_equal <<~TABLE, plain(box('list', 'containers', '-x').first)
      NAME  IMAGE       USER    STATUS                   IP          PORTS         MOUNTS
      web   nginx:1.27  root    Up 2 days                172.17.0.2  443/tcp       html:/srv/html
                                                                     8080->80/tcp
      app   app:dev     ubuntu  Up 45 minutes (healthy)  172.18.0.3                ~/src/app:/app
                                                         172.19.0.4                /etc/hosts:/etc/hosts:ro
    TABLE
  end

  def test_list_containers_leaves_out_those_gone_since_ps
    running('web' => details, 'gone' => details)
    # Docker inspect prints the ones it found, then fails for the rest
    respond('inspect', 'web-id', 'gone-id', stdout: JSON.generate([inspected('web', details.first)]), status: 1)

    stdout, stderr, status = box('list', 'containers')

    assert status.success?, stderr
    assert_equal(%w[NAME web], plain(stdout).lines.map { |line| line.split.first })
    assert_empty stderr
  end

  def test_list_containers_shows_what_the_image_sets_as_plain_text
    running('web' => details(user: "\e[31mevil", workdir: "/srv\e]0;pwned\a"))

    stdout, = box('list', 'containers')

    assert_includes stdout, "\e[90mevil\e[0m"
    assert_equal '/srv', plain(stdout).lines.last.split.last
    refute_includes stdout, "\e[31m"
    refute_includes stdout, "\e]"
  end

  def test_list_extended_containers_formats_mounts
    mounts = [
      volume('0123456789abcdef' * 4, '/cache'),
      bind("#{@home}x/other", '/other'),
      { 'Type' => 'tmpfs', 'Source' => '', 'Destination' => '/tmp', 'RW' => true },
      bind('/data', '/srv', writable: false),
    ]
    running('web' => details(workdir: '/srv', mounts: mounts))

    stdout, = box('list', 'containers', '--extended')
    shown = plain(stdout).lines.drop(1).map { |line| line.split.last }

    assert_equal ['0123456789ab:/cache', "#{@home}x/other:/other", 'tmpfs:/tmp', '/data:/srv:ro'], shown
    # Gray mounts, the working directory in the text color
    assert_includes stdout, "\e[90m/data:\e[0m/srv\e[90m:ro\e[0m"
  end

  def test_list_uses_the_settings_theme
    running('web' => details(workdir: '/srv', mounts: [bind('/data', '/srv', writable: false)]))
    settings("[theme]\nmounts = \"default\"\nworkdir = \"magenta\"\nname = 208\n")

    stdout, = box('list', 'containers', '--extended')

    assert_includes stdout, "/data:\e[35m/srv\e[0m:ro"
    assert_includes stdout, "\e[38;5;208mweb\e[0m"
  end

  def test_list_fails_on_a_wrong_theme
    settings("[theme]\nimage = \"pink\"\n")

    _, stderr, status = box('list', 'containers')

    refute status.success?
    assert_match(%r{\Abox: invalid .*/box/settings.toml: .*unknown color pink}m, stderr)
    assert_empty docker_calls
  end

  def test_list_images_prints_a_table
    containers('web' => [image_id('a')], 'worker' => [image_id('a')], 'builder' => [image_id('c')])
    respond('image', 'ls', '--no-trunc', '--format', stdout: <<~IMAGES)
      nginx\t1.27\t#{image_id('a')}\t193MB\t5 minutes ago
      app\tdev\t#{image_id('b')}\t1.23GB\t3 days ago
      <none>\t<none>\t#{image_id('c')}\t1MB\t13 months ago
    IMAGES

    stdout, = box('list', 'images')

    assert_equal <<~TABLE, plain(stdout)
      IMAGE       ID              SIZE        CREATED  CONTAINERS
      nginx:1.27  aaaaaaaaaaaa   193MB  5 minutes ago           2
      app:dev     bbbbbbbbbbbb  1.23GB     3 days ago
      <none>      cccccccccccc     1MB  13 months ago           1
    TABLE
  end

  def test_list_volumes_prints_a_table
    containers('db' => [image_id('a'), 'pgdata cache'], 'cron' => [image_id('a'), 'cache bind:'])
    volumes(
      'pgdata' => volume_details(created_at: '2026-10-15T09:30:00Z'),
      'cache' => volume_details(driver: 'nfs', created_at: ''),
      'logs' => volume_details(created_at: '2026-10-17T23:05:00+02:00'),
    )

    stdout, = box('list', 'volumes', env: { 'TZ' => 'UTC' })

    assert_equal <<~TABLE, plain(stdout)
      VOLUME  DRIVER           CREATED  CONTAINERS
      pgdata  local   2026-10-15 09:30           1
      cache   nfs                                2
      logs    local   2026-10-17 21:05
    TABLE
  end

  def test_lists_print_nothing_when_empty
    running

    %w[containers images volumes].each do |target|
      stdout, _, status = box('list', target)

      assert status.success?, target
      assert_empty stdout, target
    end
  end

  private

  def box(*, path: "#{FAKE_BIN}:#{ENV.fetch('PATH')}", env: {})
    File.write(File.join(@tmp_dir, 'responses.json'), JSON.generate(@responses))
    # Without XDG_CONFIG_HOME, so the settings are the ones under HOME
    env = { 'PATH' => path, 'HOME' => @home, 'XDG_CONFIG_HOME' => nil, 'FAKE_DOCKER_DIR' => @tmp_dir, **env }
    Open3.capture3(env, BOX, *)
  end

  def settings(toml)
    directory = File.join(@home, '.config', 'box')
    FileUtils.mkdir_p(directory)
    File.write(File.join(directory, 'settings.toml'), toml)
  end

  def respond(*arguments, stdout: '', status: 0)
    @responses << { args: arguments, stdout: stdout, status: status }
  end

  def image_id(seed)
    "sha256:#{seed * 64}"
  end

  # Every container mapped to its image ID and its mounts: volume names, or `type:name` for the other types;
  # the container names only label them
  def containers(usage)
    respond('ps', '-aq', stdout: usage.keys.map { |name| "#{name}-id\n" }.join)
    respond('inspect', '--format', '{{.Image}}', stdout: usage.values.map { |image, _| "#{image}\n" }.join)
    mounts = usage.values.map do |_, names|
      "#{names.to_s.split.map { |name| name.include?(':') ? "#{name} " : "volume:#{name} " }.join}\n"
    end
    respond('inspect', '--format', '{{range .Mounts}}{{.Type}}:{{.Name}} {{end}}', stdout: mounts.join)
  end

  # Volumes mapped to their docker volume inspect output
  def volumes(details)
    respond('volume', 'ls', '-q', stdout: details.keys.map { |name| "#{name}\n" }.join)
    respond('volume', 'inspect', stdout: details.map { |name, line| "#{name}\t#{line}\n" }.join)
  end

  def volume_details(driver: 'local', created_at: '2026-10-15T09:30:00Z')
    "#{driver}\t#{created_at}"
  end

  # Running containers, either names or names mapped to their details, with IDs like `web-id`
  def running(*containers)
    containers = containers.first.is_a?(Hash) ? containers.first : containers.to_h { |name| [name, details] }
    respond('ps', '-q', '--no-trunc', stdout: containers.keys.map { |name| "#{name}-id\n" }.join)
    ps = containers.map do |name, (_, status)|
      "#{JSON.generate('ID' => "#{name}-id", 'Names' => name, 'Status' => status)}\n"
    end
    respond('ps', '--no-trunc', '--format', '{{json .}}', stdout: ps.join)
    respond('inspect', stdout: JSON.generate(containers.map { |name, (inspect, _)| inspected(name, inspect) }))
  end

  # One container's docker inspect output, without its ID and name, and its docker ps status
  def details(ips: ['172.17.0.2'], image: 'nginx:1.27', user: '', status: 'Up 5 minutes', ports: {}, mounts: [],
              workdir: '')
    networks = ips.each_with_index.to_h { |ip, index| ["network#{index}", { 'IPAddress' => ip }] }
    inspect = {
      'Config' => { 'Image' => image, 'User' => user, 'WorkingDir' => workdir },
      'State' => { 'Status' => 'running' },
      'NetworkSettings' => { 'Networks' => networks, 'Ports' => ports },
      'Mounts' => mounts,
    }
    [inspect, status]
  end

  def inspected(name, inspect)
    { 'Id' => "#{name}-id", 'Name' => "/#{name}", **inspect }
  end

  def volume(name, destination)
    { 'Type' => 'volume', 'Name' => name, 'Destination' => destination, 'RW' => true }
  end

  def bind(source, destination, writable: true)
    { 'Type' => 'bind', 'Source' => source, 'Destination' => destination, 'RW' => writable }
  end

  def docker_calls
    path = File.join(@tmp_dir, 'calls.jsonl')
    File.exist?(path) ? File.readlines(path).map { |line| JSON.parse(line) } : []
  end

  # Calls that change something
  def changes
    docker_calls.select { |call| %w[rm volume rmi stop].include?(call.first) && !call.include?('-q') }
  end

  def plain(output)
    output.gsub(/\e\[[\d;]*m/, '')
  end
end
