defmodule App do
  def run() do
    hostname = Enum.at String.split(to_string(node()), "@"), 1
    remote_process = String.to_atom ~s"edp-example-reverse-string@#{hostname}"
    IO.puts ~s"Remote process is #{to_string(remote_process)}."
    start_time = DateTime.utc_now()
    thread_count = 100
    loop_count = 10000

    msg = to_string(Enum.map(0..127, fn x -> ?a + rem(x, 26) end))
    msg_rev = to_string(Enum.reverse(to_charlist(msg)))

    handles = for i <- 0..thread_count do
      Task.async fn ->
        start_time = DateTime.utc_now()
        for _ <- 0..loop_count do
          send {:any, remote_process}, msg
          got = receive do
            x when is_binary(x) -> x
          after
            1000 ->
              raise "rpc timeout"
          end
          if got != msg_rev do
            IO.inspect got
            raise "Invalid response"
          end
        end
        end_time = DateTime.utc_now()
        time_diff = DateTime.to_unix(end_time, :microsecond) - DateTime.to_unix(start_time, :microsecond)
        time_diff / loop_count # latency
      end
    end
    {lat_sum, lat_count} = Enum.reduce(handles, {0, 0}, fn (x, {sum, count}) -> {sum + Task.await(x, :infinity), count + 1} end)
    avg_lat = lat_sum / lat_count
    end_time = DateTime.utc_now()
    time_diff = DateTime.to_unix(end_time, :microsecond) - DateTime.to_unix(start_time, :microsecond)

    IO.puts ~s"Bandwidth: #{(loop_count * thread_count / time_diff) * 1000000} op/s"
    IO.puts ~s"Latency: #{round(avg_lat * 1000)}ns"
  end
end

App.run()
