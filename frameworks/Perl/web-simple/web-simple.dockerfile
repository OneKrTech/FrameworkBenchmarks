FROM perl:5.40

RUN apt-get update -yqq && apt-get install -yqq nginx

WORKDIR /simple

RUN cpanm --notest --no-man-page  \
        JSON JSON::XS IO::Socket::IP IO::Socket::SSL \
        Web::Simple@0.033 \
        DBI@1.637 \
        DBD::MariaDB@1.23 \
        Plack@1.0044 \
        Starman@0.4014 \
        JSON::XS@3.04

ADD ./conf /simple/
ADD ./app.pl /simple/
ADD ./nginx.conf /simple/

EXPOSE 8080

CMD nginx -c /simple/nginx.conf && \
    plackup -E production -s Starman --workers=$(nproc) \
    --max-requests=100000 \
    -l /tmp/perl-simple.sock -a /simple/app.pl

