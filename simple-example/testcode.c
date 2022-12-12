#include "confuse_ll.h"
#include "confuse_dio.h"
#include <time.h>
#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>
#include <string.h>

//borrowed from https://codereview.stackexchange.com/questions/29198/random-string-generator-in-c
static char *rand_string(char *str, size_t size)
{
    const char charset[] = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789,.-#'?!";
    if (size) {
        --size;
        for (size_t n = 0; n < size; n++) {
            int key = rand() % (int) (sizeof charset - 1);
            str[n] = charset[key];
        }
        str[size] = '\0';
    }
    return str;
}

int main(int argc, char** argv) {

  simics_handle simics;
  int failcnt=0;
  
  if (argc != 2) {
      printf("Please provide a path to a Simics project as an argument.\n");
      exit(1);
  }
  
  
  unsigned char* shm_array = confuse_create_dio_shared_mem(16*1024*1024);
  if (shm_array == NULL) {
      printf("Could not create shm.\n");
      exit(-1);
  }
  
  int rv = confuse_init(argv[1], "simple-example/simics-scripts/qsp-x86-uefi-app.yml", &simics);
  if (rv) {
      printf("Could not initialize Simics.\n");
      exit(-1);
  }
  
  
  struct timespec start, stop;
  printf("Loop start\n");
  clock_gettime(CLOCK_REALTIME, &start);
  for (int i = 0; i < 1000; i ++) {
      //TODO: clear shared mem here if needed.
      confuse_reset(simics);
      size_t len = 20;
      rand_string(shm_array+sizeof(size_t), len); //write this into shm
      memcpy(shm_array, &len, sizeof(size_t));
      
      //TODO: Setup input data etc
      //      Maybe based on some predefined format that has
      //        - data for specific mem regions
      //        - data for specific registers
      //        - data that needs to be pushed into magic pipe
      confuse_run(simics);

      //printf("Iteration %d %s\n", i, shm_array+sizeof(size_t)); //right now we know that we just get a string
      if (strcmp(shm_array+sizeof(size_t), "Fail") == 0) failcnt++;
      //sleep(1);
  }
  clock_gettime(CLOCK_REALTIME, &stop);
  
  double duration = (stop.tv_sec - start.tv_sec) +
                    (stop.tv_nsec - start.tv_nsec) / 1000000000.0;
  
  printf("Total duration %lf with %d failures \n", duration, failcnt);
  return 0;

}

